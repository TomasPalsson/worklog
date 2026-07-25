//! Billing export — turn a day's blocks into the line items the
//! external invoicing system is filled in from.
//!
//! One row maps 1:1 onto one submission of the invoicing form:
//!
//! | form field          | source                                            |
//! |---------------------|---------------------------------------------------|
//! | Dagsetning          | the day being exported                            |
//! | Viðskiptamaður      | folder pin, else customer alias found in the text |
//! | Verkefni (deild)    | folder pin only — **never** guessed               |
//! | Tegund skráningar   | [`TEGUND_SKRANINGAR`] (constant)                  |
//! | Taxti               | [`TAXTI`] (constant)                              |
//! | Tímar               | overlap-safe union of block time, ½h-rounded       |
//! | Reikningshæfi       | folder pin, default Reikningshæft                 |
//! | Texti á reikning    | the block description, as-is                      |
//!
//! Two rules keep this trustworthy: nothing that would land on an
//! invoice is invented (an unresolved field comes out `None` for the
//! user to fill in), and hours are the **union** of a group's intervals
//! so overlapping blocks are never double-billed.
//!
//! Personal blocks are excluded outright — that time never goes into the
//! invoicing system.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params_from_iter, Connection};

use crate::billing_registry::Registry;
use crate::collectors::tempo::{round_to_half_hour, HALF_HOUR_SECONDS};
use crate::models::Block;
use crate::repo;

/// Shown wherever a field could not be resolved and the user must pick
/// it in the form.
pub const BLANK: &str = "—";

/// `Tegund skráningar` — always a plain registration; the driving and
/// on-call variants are never billed by this user.
pub const TEGUND_SKRANINGAR: &str = "Almenn skráning";

/// `Taxti` — always day rate.
pub const TAXTI: &str = "Dagvinna";

/// The two `Reikningshæfi` values.
pub const REIKNINGSHAEFT: &str = "Reikningshæft";
pub const OREIKNINGSHAEFT: &str = "Óreikningshæft";

/// `Reikningshæfi` label for a billable flag.
pub fn reikningshaefi(billable: bool) -> &'static str {
    if billable {
        REIKNINGSHAEFT
    } else {
        OREIKNINGSHAEFT
    }
}

/// One line item — one submission of the invoicing form.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BillingRow {
    /// ISO `YYYY-MM-DD`. Rendered as `dd.mm.yyyy` for the form.
    pub day: String,
    /// Resolved work folder. Context for the user (and the key the
    /// registry maps), not itself a form field.
    pub folder: String,
    /// `Viðskiptamaður`; `None` when undetectable — the user fills it in.
    pub customer: Option<String>,
    /// `Verkefni (deild)`; `None` unless a folder pin supplied it.
    pub verkefni: Option<String>,
    /// The Jira key this line came from, when there was one. Context
    /// only — helps the user recognise the line.
    pub ticket: Option<String>,
    /// Overlap-safe union of the group's block intervals, unrounded.
    pub seconds: i64,
    /// `Tímar` — `seconds` rounded to the nearest half hour.
    pub hours: f64,
    /// `Reikningshæfi` as a bool; `true` = Reikningshæft.
    pub billable: bool,
    /// `Texti á reikning` — the block description, unmodified.
    pub invoice_text: String,
    /// True when no block in the group had a description, so
    /// `invoice_text` is a fallback (an event title, or `Work in
    /// {folder}`) rather than real work text.
    ///
    /// Surfaced so the UI can say *why* a line reads "Work in sjukra":
    /// the day hasn't been through `worklog estimate` yet. Without this
    /// the fallback looks like the export ignoring the descriptions it
    /// was supposed to use.
    pub needs_description: bool,
    /// How many blocks folded into this line.
    pub block_count: i64,
    /// Earliest block start and latest block end in the group (ISO-8601).
    /// Shown so the user can place the work in their day.
    pub started_at: String,
    pub ended_at: String,
    /// Distinct `project_path`s behind the line, most-used first (capped).
    /// The single most useful hint for guessing a blank Viðskiptamaður —
    /// the folder name alone often isn't enough to recognise the work.
    pub paths: Vec<String>,
    /// Ids of the blocks folded into this line, so the review UI can group
    /// a day exactly the way the export bills it instead of by Jira ticket.
    pub block_ids: Vec<i64>,
}

impl BillingRow {
    /// `Dagsetning` as the form wants it: `dd.mm.yyyy`.
    pub fn date_display(&self) -> String {
        match chrono::NaiveDate::parse_from_str(&self.day, "%Y-%m-%d") {
            Ok(d) => d.format("%d.%m.%Y").to_string(),
            // Unparseable day: show it verbatim rather than inventing one.
            Err(_) => self.day.clone(),
        }
    }

    /// `Tímar` as the form wants it: comma decimal, no trailing `,0`.
    pub fn hours_display(&self) -> String {
        format_hours(self.seconds)
    }

    /// True when the user still has to pick something for this line.
    pub fn needs_input(&self) -> bool {
        self.customer.is_none() || self.verkefni.is_none()
    }
}

/// Output format for [`render`]. Adding a format is a new variant plus a
/// matching `render` arm — row computation never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Csv,
    Json,
}

/// The work-folder (project root) a `project_path` belongs to.
///
/// Two normalisations matter, because most real paths are neither plain
/// nor stable:
///
/// * **worktrees** live at `<root>/.claude/worktrees/<name>`, so
///   everything from `/.claude/` on is stripped — otherwise
///   `sjukra/.claude/worktrees/mega-audit` would bill as `mega-audit`.
/// * **sub-directories** collapse to the project root: the first segment
///   beneath the work prefix wins, so `sjukra/app` is still `sjukra`.
///
/// Paths outside the work prefix fall back to their last segment, which
/// keeps explicitly-configured work paths outside `~/Desktop/Work`
/// working. `None` only for an unusable path.
pub fn work_folder_for_path(path: &str) -> Option<String> {
    // Drop the worktree / agent scaffolding.
    let base = match path.find("/.claude/") {
        Some(i) => &path[..i],
        None => path,
    };
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        return None;
    }

    if let Some(prefix) = work_prefix() {
        if let Some(rest) = base.strip_prefix(prefix) {
            let rest = rest.trim_start_matches('/');
            if rest.is_empty() {
                // The work root itself is not a project — returning its
                // basename here produced the nonsense folder "Work".
                return None;
            }
            let first = rest.split('/').next().unwrap_or(rest);
            if !first.is_empty() {
                return Some(first.to_owned());
            }
        }
    }

    base.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The work folder for `path`, but **only** when `path` actually lives
/// under the work prefix.
///
/// [`work_folder_for_path`] is deliberately lenient — it falls back to the
/// last path segment so explicitly-configured work paths outside
/// `~/Desktop/Work` still attribute. That leniency is wrong for
/// *discovery*: it turns `~/dotfiles` and `~/Desktop/Projects/exam` into
/// candidate billing folders, cluttering the "unmapped folders" list with
/// personal work that can never be billed.
pub fn billable_work_folder(path: &str) -> Option<String> {
    let base = match path.find("/.claude/") {
        Some(i) => &path[..i],
        None => path,
    };
    let prefix = work_prefix()?;
    let rest = base.trim_end_matches('/').strip_prefix(prefix)?;
    let rest = rest.trim_start_matches('/');
    rest.split('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// `~/Desktop/Work`, expanded. Mirrors `personal::default_work_prefix` —
/// the same prefix that decides work-vs-personal decides which path
/// segment is the billable project root.
///
/// Computed once: the folder-resolution helpers run per path, and resolving
/// the home directory on every call turned a hot loop into a syscall storm.
fn work_prefix() -> Option<&'static str> {
    static PREFIX: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    PREFIX
        .get_or_init(|| {
            dirs::home_dir().map(|mut p| {
                p.push("Desktop/Work");
                p.to_string_lossy().into_owned()
            })
        })
        .as_deref()
}

/// Most-frequent work folder across a block's events, derived from
/// `events.project_path` (with `events.repo` as a fallback for blocks
/// whose events only carry a GitHub repo). `None` when the block has
/// neither — e.g. a pure calendar or Jira block.
pub fn work_folder_for_block(conn: &Connection, block_id: i64) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT e.project_path, e.repo
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id = ?1",
    )?;
    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map([block_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut counts: HashMap<String, u32> = HashMap::new();
    for (project_path, repo_name) in rows {
        let key = project_path
            .as_deref()
            .and_then(work_folder_for_path)
            // A GitHub repo like `aproorg/LibreChat` → `LibreChat`.
            .or_else(|| {
                repo_name
                    .as_deref()
                    .and_then(|r| r.rsplit('/').next())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            });
        if let Some(key) = key {
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    // Ties break lexicographically so a day's rows are reproducible.
    let mut ranked: Vec<(String, u32)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(ranked.into_iter().next().map(|(k, _)| k))
}

/// Most-frequent non-empty `events.title` across a set of blocks'
/// events, considering only sources whose title describes *work*. Ties
/// break lexicographically for determinism.
///
/// Claude-Code hook events are excluded on purpose. Their title is the
/// hook name (`PreToolUse`, `SessionEnd`, …), which is meaningless as
/// invoice text — and worse, some carry captured terminal content
/// (`UserPromptSubmit — … aws dynamodb query --region …`). Neither
/// belongs on a customer's invoice, so only commit / PR / calendar /
/// ticket titles are eligible.
fn dominant_title_for_blocks(conn: &Connection, block_ids: &[i64]) -> Result<Option<String>> {
    if block_ids.is_empty() {
        return Ok(None);
    }
    let placeholders = vec!["?"; block_ids.len()].join(",");
    let sql = format!(
        "SELECT e.title
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id IN ({placeholders})
            AND e.source NOT LIKE 'claude%'"
    );
    let mut stmt = conn.prepare(&sql)?;
    let titles: Vec<String> = stmt
        .query_map(params_from_iter(block_ids.iter()), |r| {
            r.get::<_, String>(0)
        })?
        .collect::<std::result::Result<_, _>>()?;

    let mut counts: HashMap<String, u32> = HashMap::new();
    for title in titles {
        let title = title.trim();
        if !title.is_empty() {
            *counts.entry(title.to_string()).or_insert(0) += 1;
        }
    }
    let mut ranked: Vec<(String, u32)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(ranked.into_iter().next().map(|(title, _)| title))
}

/// Distinct `project_path`s across a set of blocks' events, most-used
/// first, capped so a busy group can't produce an unbounded list.
///
/// Aggregated in SQL rather than in Rust because a single block can carry
/// thousands of Claude hook events.
fn distinct_paths_for_blocks(conn: &Connection, block_ids: &[i64]) -> Result<Vec<String>> {
    if block_ids.is_empty() {
        return Ok(Vec::new());
    }
    const MAX_PATHS: usize = 4;
    let placeholders = vec!["?"; block_ids.len()].join(",");
    let sql = format!(
        "SELECT e.project_path, COUNT(*) n
           FROM events e
           JOIN block_events be ON be.event_id = e.id
          WHERE be.block_id IN ({placeholders})
            AND e.project_path IS NOT NULL
          GROUP BY e.project_path
          ORDER BY n DESC, e.project_path
          LIMIT {MAX_PATHS}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let paths = stmt
        .query_map(params_from_iter(block_ids.iter()), |r| {
            r.get::<_, String>(0)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(paths)
}

/// The Jira ticket summary for a block's ticket, when both exist. Feeds
/// the customer alias match — summaries like "Document analyzer fyrir
/// Sjúkra" are where the customer actually appears.
fn ticket_summary(conn: &Connection, ticket: Option<&str>) -> Result<Option<String>> {
    let Some(key) = ticket.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            "SELECT summary FROM jira_tickets WHERE key = ?1",
            [key],
            |r| r.get::<_, String>(0),
        )
        .ok())
}

/// A block's wall-clock interval as epoch seconds: `[start, start +
/// duration)`. Duration — not `ended_at` — is the canonical logged time,
/// matching `worklog_cli::cli::block_interval`.
fn block_interval(block: &Block) -> (i64, i64) {
    let start = chrono::DateTime::parse_from_rfc3339(&block.started_at)
        .map(|d| d.timestamp())
        .unwrap_or(0);
    (start, start + block.duration_seconds.max(0))
}

/// Total length of the union of `[start, end)` intervals — stretches
/// covered twice count once, so a meeting held during coding is never
/// double-billed. Mirrors `worklog_cli::cli::union_seconds`.
fn union_seconds(mut intervals: Vec<(i64, i64)>) -> i64 {
    intervals.sort_by_key(|&(s, _)| s);
    let mut total = 0;
    let mut cur: Option<(i64, i64)> = None;
    for (s, e) in intervals {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

/// What distinguishes one line item from another: the Jira ticket if there
/// is one, else the work folder.
///
/// Deliberately **not** the description. Keying on description meant every
/// distinct sentence became its own invoice line — a single estimated day
/// produced 13 lines, seven of them the same customer at half an hour each,
/// which is 13 trips through the invoicing form. Work in one folder for one
/// customer is one line; the individual descriptions are joined into its
/// `Texti á reikning`, so nothing is lost, it just reads as
/// "fixed this; fixed that" the way a real invoice line does.
///
/// Distinct tickets still split, because a ticket is a real billing
/// boundary in a way a sentence is not.
fn task_for_block(block: &Block, folder: &str) -> String {
    if let Some(issue) = block.jira_issue.as_deref().filter(|s| !s.is_empty()) {
        return issue.to_string();
    }
    folder.to_owned()
}

/// Accumulator for one `(customer, verkefni, task)` group.
struct GroupAcc {
    folder: String,
    customer: Option<String>,
    verkefni: Option<String>,
    ticket: Option<String>,
    billable: bool,
    intervals: Vec<(i64, i64)>,
    /// Distinct non-empty descriptions in first-seen order.
    descriptions: Vec<String>,
    block_ids: Vec<i64>,
    /// Raw block bounds, so the line can report the span it covers.
    starts: Vec<String>,
    ends: Vec<String>,
}

/// Longest `Texti á reikning` we will build by joining a group's distinct
/// block descriptions. Now that a line is "everything I did in this folder
/// for this customer", the joined list *is* the invoice text, so the cap is
/// generous enough that a normal day shows every item — but still bounded,
/// because 20+ blocks on one ticket would otherwise be a paragraph.
const INVOICE_TEXT_MAX: usize = 400;

/// Join a group's distinct descriptions into one invoice line, stopping
/// before it becomes a wall of text and saying how many were left out.
///
/// Honest truncation over silent truncation: `(+N more)` tells the user
/// there is detail they may want to summarise themselves, rather than
/// implying the line is the whole story.
fn join_descriptions(descriptions: &[String]) -> String {
    let mut out = String::new();
    let mut used = 0;
    for d in descriptions {
        // Always take the first, even if it alone exceeds the cap — a line
        // must never be empty.
        if used > 0 && out.chars().count() + d.chars().count() + 2 > INVOICE_TEXT_MAX {
            break;
        }
        if used > 0 {
            out.push_str("; ");
        }
        out.push_str(d);
        used += 1;
    }
    let omitted = descriptions.len() - used;
    if omitted > 0 {
        out.push_str(&format!(" (+{omitted} more)"));
    }
    out
}

/// Deterministic, never-empty invoice text for a group whose blocks all
/// lack a description: the group's most-frequent event title, else the
/// folder name, else a neutral placeholder.
fn fallback_invoice_text(conn: &Connection, acc: &GroupAcc) -> Result<String> {
    if let Some(title) = dominant_title_for_blocks(conn, &acc.block_ids)? {
        return Ok(title);
    }
    Ok(if acc.folder == BLANK {
        "Unspecified work".to_string()
    } else {
        format!("Work in {}", acc.folder)
    })
}

/// Compute a day's billing rows.
///
/// Personal blocks are skipped entirely. Each remaining block resolves
/// its folder → customer/verkefni/billable through the [`Registry`],
/// then blocks are grouped by `(customer, verkefni, task)`. A group's
/// `seconds` is the **union** of its blocks' intervals (never a naive
/// sum) and `hours` is that union rounded to the nearest half hour.
///
/// Rows sort with the lines still needing input first (so the user sees
/// what to fill), then by customer, then by descending time.
pub fn rows_for_day(conn: &Connection, day: &str) -> Result<Vec<BillingRow>> {
    let registry = Registry::load(conn)?;
    let blocks = repo::list_blocks_for_day(conn, day)?;

    type Key = (String, String, String);
    let mut groups: HashMap<Key, GroupAcc> = HashMap::new();
    let mut order: Vec<Key> = Vec::new();

    for block in blocks.iter() {
        // Personal time never reaches the invoicing system.
        if block.is_personal {
            continue;
        }

        let folder = work_folder_for_block(conn, block.id)?.unwrap_or_else(|| BLANK.to_string());
        let ticket = block
            .jira_issue
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        // The text the customer alias match runs against: the ticket
        // summary (where customers are usually named) plus this block's
        // own description.
        let mut haystack = String::new();
        if let Some(summary) = ticket_summary(conn, ticket.as_deref())? {
            haystack.push_str(&summary);
            haystack.push('\n');
        }
        if let Some(desc) = block.description.as_deref() {
            haystack.push_str(desc);
        }
        let resolved = registry.resolve(&folder, &haystack);

        let key = (
            resolved.customer.clone().unwrap_or_default(),
            resolved.verkefni.clone().unwrap_or_default(),
            task_for_block(block, &folder),
        );

        if !groups.contains_key(&key) {
            order.push(key.clone());
            groups.insert(
                key.clone(),
                GroupAcc {
                    folder,
                    customer: resolved.customer,
                    verkefni: resolved.verkefni,
                    ticket,
                    billable: resolved.billable,
                    intervals: Vec::new(),
                    descriptions: Vec::new(),
                    block_ids: Vec::new(),
                    starts: Vec::new(),
                    ends: Vec::new(),
                },
            );
        }
        let acc = groups.get_mut(&key).expect("group just inserted");
        acc.intervals.push(block_interval(block));
        acc.block_ids.push(block.id);
        acc.starts.push(block.started_at.clone());
        acc.ends.push(block.ended_at.clone());
        if let Some(desc) = block
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !acc.descriptions.iter().any(|d| d == desc) {
                acc.descriptions.push(desc.to_string());
            }
        }
    }

    let mut rows: Vec<BillingRow> = order
        .into_iter()
        .map(|key| -> Result<BillingRow> {
            let acc = groups.remove(&key).expect("group present for its own key");
            let needs_description = acc.descriptions.is_empty();
            let invoice_text = if needs_description {
                fallback_invoice_text(conn, &acc)?
            } else {
                join_descriptions(&acc.descriptions)
            };
            let block_count = acc.block_ids.len() as i64;
            let started_at = acc.starts.iter().min().cloned().unwrap_or_default();
            let ended_at = acc.ends.iter().max().cloned().unwrap_or_default();
            let paths = distinct_paths_for_blocks(conn, &acc.block_ids)?;
            let seconds = union_seconds(acc.intervals);
            Ok(BillingRow {
                day: day.to_owned(),
                folder: acc.folder,
                customer: acc.customer,
                verkefni: acc.verkefni,
                ticket: acc.ticket,
                seconds,
                hours: round_to_half_hour(seconds) as f64 / 3600.0,
                billable: acc.billable,
                invoice_text,
                needs_description,
                block_count,
                started_at,
                ended_at,
                paths,
                block_ids: acc.block_ids,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // A group that rounds to 0 hours cannot be billed, and a wizard full
    // of "0 hrs" lines is pure noise. Tempo sync already skipped
    // sub-half-hour groups, so this keeps the two consistent.
    rows.retain(|r| round_to_half_hour(r.seconds) > 0);

    rows.sort_by(|a, b| {
        b.needs_input()
            .cmp(&a.needs_input())
            .then_with(|| a.customer.cmp(&b.customer))
            .then(b.seconds.cmp(&a.seconds))
    });
    Ok(rows)
}

/// Render rounded seconds as comma-decimal hours using integer
/// half-hour math — guarantees `5,5` / `4` with no float noise.
fn format_hours(seconds: i64) -> String {
    let halves = round_to_half_hour(seconds) / HALF_HOUR_SECONDS;
    let whole = halves / 2;
    if halves % 2 == 1 {
        format!("{whole},5")
    } else {
        format!("{whole}")
    }
}

fn or_blank(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or(BLANK)
}

/// Text: one aligned line per row, in form order, for scanning in a
/// terminal. Unresolved fields show [`BLANK`].
fn render_text(rows: &[BillingRow]) -> String {
    let col_width = |f: &dyn Fn(&BillingRow) -> String| {
        rows.iter().map(|r| f(r).chars().count()).max().unwrap_or(0)
    };
    let cw = col_width(&|r: &BillingRow| or_blank(&r.customer).to_owned());
    let vw = col_width(&|r: &BillingRow| or_blank(&r.verkefni).to_owned());
    let hw = col_width(&|r: &BillingRow| r.hours_display());

    let pad = |s: &str, w: usize| {
        let n = s.chars().count();
        format!("{s}{}", " ".repeat(w.saturating_sub(n)))
    };

    rows.iter()
        .map(|r| {
            format!(
                "{}  {}  {}  {} hrs  {}  {}",
                r.date_display(),
                pad(or_blank(&r.customer), cw),
                pad(or_blank(&r.verkefni), vw),
                pad(&r.hours_display(), hw),
                reikningshaefi(r.billable),
                r.invoice_text,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// NFR 5.2 CSV formula-injection guard: prefixes `'` when the first
/// character could make a spreadsheet treat the cell as a formula.
fn csv_guard(field: &str) -> String {
    match field.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r') => {
            format!("'{field}")
        }
        _ => field.to_string(),
    }
}

/// RFC-4180 quoting: wrap in `"` (doubling internal `"`) when the field
/// contains a comma, quote, CR or LF.
fn csv_quote(field: &str) -> String {
    if field.contains(['"', ',', '\r', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn csv_cell(field: &str) -> String {
    csv_quote(&csv_guard(field))
}

/// CSV in the invoicing form's field order. Blanks are truly empty cells
/// (not `—`) so a spreadsheet import doesn't inherit a dash.
fn render_csv(rows: &[BillingRow]) -> String {
    let mut lines = vec![
        "dagsetning,vidskiptamadur,verkefni,tegund_skraningar,taxti,timar,reikningshaefi,texti_a_reikning"
            .to_string(),
    ];
    for r in rows {
        lines.push(
            [
                csv_cell(&r.date_display()),
                csv_cell(r.customer.as_deref().unwrap_or("")),
                csv_cell(r.verkefni.as_deref().unwrap_or("")),
                csv_cell(TEGUND_SKRANINGAR),
                csv_cell(TAXTI),
                csv_quote(&r.hours_display()),
                csv_cell(reikningshaefi(r.billable)),
                csv_cell(&r.invoice_text),
            ]
            .join(","),
        );
    }
    lines.join("\n")
}

/// One JSON line item. `null` (not `"—"`) for unresolved fields so a
/// consumer can tell "not known" from a literal dash.
#[derive(serde::Serialize)]
struct JsonRow<'a> {
    dagsetning: String,
    vidskiptamadur: Option<&'a str>,
    verkefni: Option<&'a str>,
    tegund_skraningar: &'static str,
    taxti: &'static str,
    timar: f64,
    reikningshaefi: &'static str,
    texti_a_reikning: &'a str,
    // Context, not form fields.
    day: &'a str,
    folder: &'a str,
    ticket: Option<&'a str>,
    seconds: i64,
    block_count: i64,
    paths: &'a [String],
}

fn render_json(rows: &[BillingRow]) -> String {
    let json_rows: Vec<JsonRow> = rows
        .iter()
        .map(|r| JsonRow {
            dagsetning: r.date_display(),
            vidskiptamadur: r.customer.as_deref(),
            verkefni: r.verkefni.as_deref(),
            tegund_skraningar: TEGUND_SKRANINGAR,
            taxti: TAXTI,
            timar: r.hours,
            reikningshaefi: reikningshaefi(r.billable),
            texti_a_reikning: &r.invoice_text,
            day: &r.day,
            folder: &r.folder,
            ticket: r.ticket.as_deref(),
            seconds: r.seconds,
            block_count: r.block_count,
            paths: &r.paths,
        })
        .collect();
    serde_json::to_string_pretty(&json_rows)
        .expect("billing rows are plain data and always serialize")
}

/// Render `rows` in `format`. All formats render from the same row slice
/// — see [`render_text`], [`render_csv`], [`render_json`].
pub fn render(rows: &[BillingRow], format: Format) -> String {
    match format {
        Format::Text => render_text(rows),
        Format::Csv => render_csv(rows),
        Format::Json => render_json(rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing_registry::{upsert_customer, upsert_folder, Customer, FolderMap};
    use crate::db::open_memory;
    use crate::models::Event;
    use crate::repo as repository;
    use rusqlite::params;

    fn home() -> String {
        dirs::home_dir().unwrap().to_string_lossy().into_owned()
    }

    fn work(sub: &str) -> String {
        format!("{}/Desktop/Work/{sub}", home())
    }

    #[allow(clippy::too_many_arguments)]
    fn seed_block(
        conn: &Connection,
        started_at: &str,
        duration_seconds: i64,
        jira_issue: Option<&str>,
        description: Option<&str>,
        is_personal: bool,
    ) -> i64 {
        conn.execute(
            "INSERT INTO blocks
                (day, jira_issue, started_at, ended_at, duration_seconds, description, is_personal)
             VALUES ('2026-07-23', ?1, ?2, ?2, ?3, ?4, ?5)",
            params![
                jira_issue,
                started_at,
                duration_seconds,
                description,
                is_personal as i64
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_event(
        conn: &Connection,
        block_id: i64,
        source_id: &str,
        project_path: Option<&str>,
        title: &str,
    ) {
        let mut ev = Event::minimal("claude", source_id, "2026-07-23T09:00:00Z", title);
        ev.project_path = project_path.map(str::to_string);
        let eid = repository::upsert_event(conn, &ev).unwrap();
        conn.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![block_id, eid],
        )
        .unwrap();
    }

    fn pin(conn: &Connection, folder: &str, customer: Option<&str>, verkefni: Option<&str>) {
        upsert_folder(
            conn,
            &FolderMap {
                id: None,
                folder: folder.into(),
                customer: customer.map(str::to_owned),
                verkefni: verkefni.map(str::to_owned),
                billable: true,
            },
        )
        .unwrap();
    }

    // ───────────── work_folder_for_path (the worktree/subdir fix) ─────────────

    #[test]
    fn work_folder_collapses_worktrees_to_the_project_root() {
        // Regression: the basename of a worktree path is the branch name,
        // so `sjukra/.claude/worktrees/mega-audit` used to bill as
        // "mega-audit" and fragment the customer's day.
        assert_eq!(
            work_folder_for_path(&work("sjukra/.claude/worktrees/mega-audit")),
            Some("sjukra".into())
        );
    }

    #[test]
    fn work_folder_collapses_subdirs_to_the_project_root() {
        assert_eq!(
            work_folder_for_path(&work("sjukra/app")),
            Some("sjukra".into())
        );
        assert_eq!(
            work_folder_for_path(&work("claude-exam/pro-practice")),
            Some("claude-exam".into())
        );
    }

    #[test]
    fn work_folder_keeps_a_plain_work_folder() {
        assert_eq!(
            work_folder_for_path(&work("genai-infra")),
            Some("genai-infra".into())
        );
    }

    #[test]
    fn work_folder_outside_the_prefix_falls_back_to_the_last_segment() {
        assert_eq!(
            work_folder_for_path("/opt/contract/acme-api"),
            Some("acme-api".into())
        );
        assert_eq!(work_folder_for_path(""), None);
    }

    #[test]
    fn worktree_and_root_events_land_in_the_same_folder() {
        let c = open_memory().unwrap();
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            3600,
            None,
            Some("w"),
            false,
        );
        seed_event(&c, b, "e1", Some(&work("sjukra")), "commit");
        seed_event(
            &c,
            b,
            "e2",
            Some(&work("sjukra/.claude/worktrees/thing")),
            "commit",
        );
        assert_eq!(
            work_folder_for_block(&c, b).unwrap(),
            Some("sjukra".into()),
            "a worktree event must not split the folder"
        );
    }

    // ────────────────────────── row computation ──────────────────────────

    #[test]
    fn personal_blocks_never_appear() {
        let c = open_memory().unwrap();
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            3600,
            None,
            Some("hobby"),
            true,
        );
        seed_event(&c, b, "e1", Some("/Users/x/Desktop/Projects/toy"), "s");
        assert!(rows_for_day(&c, "2026-07-23").unwrap().is_empty());
    }

    #[test]
    fn a_pinned_folder_fills_customer_and_verkefni() {
        let c = open_memory().unwrap();
        pin(
            &c,
            "apro-website",
            Some("APRÓ"),
            Some("Vefsíður APRÓ og dótturfélaga"),
        );
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            14400,
            None,
            Some("Website updates"),
            false,
        );
        seed_event(&c, b, "e1", Some(&work("apro-website")), "s");

        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].customer, Some("APRÓ".into()));
        assert_eq!(
            rows[0].verkefni,
            Some("Vefsíður APRÓ og dótturfélaga".into())
        );
        assert_eq!(rows[0].hours, 4.0);
        assert!(rows[0].billable);
        assert!(!rows[0].needs_input());
    }

    #[test]
    fn a_shared_folder_resolves_the_customer_from_the_ticket_summary() {
        let c = open_memory().unwrap();
        upsert_customer(
            &c,
            &Customer {
                id: None,
                name: "Sjúkra".into(),
                aliases: vec![],
            },
        )
        .unwrap();
        pin(&c, "genai-infra", None, None); // shared
        c.execute(
            "INSERT INTO jira_tickets (key, summary) VALUES ('GENAI-1219', ?1)",
            params!["Document analyzer fyrir Sjúkra"],
        )
        .unwrap();
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            7200,
            Some("GENAI-1219"),
            Some("Build the analyzer"),
            false,
        );
        seed_event(&c, b, "e1", Some(&work("genai-infra")), "s");

        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].customer, Some("Sjúkra".into()));
        // The accounting key is never guessed.
        assert_eq!(rows[0].verkefni, None);
        assert!(rows[0].needs_input());
        assert_eq!(rows[0].ticket, Some("GENAI-1219".into()));
    }

    #[test]
    fn an_unresolvable_customer_is_left_blank() {
        let c = open_memory().unwrap();
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            3600,
            None,
            Some("Mystery work"),
            false,
        );
        seed_event(&c, b, "e1", Some("/somewhere/else/mystery"), "s");
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].customer, None);
        assert!(rows[0].needs_input());
    }

    #[test]
    fn same_folder_different_descriptions_collapse_into_one_line() {
        // 13 lines for one day (7 of them the same customer at half an hour
        // each) came from keying the group on the description. Work in one
        // folder for one customer is one invoice line; the descriptions are
        // joined into its Texti á reikning.
        let c = open_memory().unwrap();
        for (i, (start, desc)) in [
            (
                "2026-07-23T09:00:00+00:00",
                "Fix the poller sentinel orphan",
            ),
            ("2026-07-23T11:00:00+00:00", "Approve PR #152 for merge"),
            ("2026-07-23T13:00:00+00:00", "Investigate case loading"),
        ]
        .iter()
        .enumerate()
        {
            let b = seed_block(&c, start, 3600, None, Some(desc), false);
            seed_event(&c, b, &format!("e{i}"), Some(&work("sjukra")), "PreToolUse");
        }
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1, "got {rows:#?}");
        assert_eq!(rows[0].block_count, 3);
        assert_eq!(rows[0].hours, 3.0);
        for want in ["poller sentinel", "PR #152", "case loading"] {
            assert!(
                rows[0].invoice_text.contains(want),
                "got {}",
                rows[0].invoice_text
            );
        }
        // Context for guessing a blank customer.
        assert!(rows[0].paths.iter().any(|p| p.contains("sjukra")));
        assert!(rows[0].started_at < rows[0].ended_at);
    }

    #[test]
    fn distinct_tickets_stay_distinct_lines() {
        let c = open_memory().unwrap();
        seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            14400,
            Some("GENAI-1"),
            Some("First task"),
            false,
        );
        seed_block(
            &c,
            "2026-07-23T14:00:00+00:00",
            19800,
            Some("GENAI-2"),
            Some("Second task"),
            false,
        );
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 2);
        let hours: Vec<String> = rows.iter().map(|r| r.hours_display()).collect();
        assert!(hours.contains(&"4".to_string()), "got {hours:?}");
        assert!(hours.contains(&"5,5".to_string()), "got {hours:?}");
    }

    #[test]
    fn same_ticket_blocks_merge_and_overlaps_count_once() {
        let c = open_memory().unwrap();
        // Two 1h blocks on one ticket overlapping by 30m → 1.5h, not 2h.
        seed_block(
            &c,
            "2026-07-23T10:00:00+00:00",
            3600,
            Some("GENAI-9"),
            Some("Work"),
            false,
        );
        seed_block(
            &c,
            "2026-07-23T10:30:00+00:00",
            3600,
            Some("GENAI-9"),
            Some("Work"),
            false,
        );
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seconds, 5400, "union, not sum");
        assert_eq!(rows[0].hours, 1.5);
    }

    #[test]
    fn billable_flag_follows_the_folder_pin() {
        let c = open_memory().unwrap();
        upsert_folder(
            &c,
            &FolderMap {
                id: None,
                folder: "internal-admin".into(),
                customer: Some("APRÓ".into()),
                verkefni: Some("[O] Innra support".into()),
                billable: false,
            },
        )
        .unwrap();
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            3600,
            None,
            Some("Admin"),
            false,
        );
        seed_event(&c, b, "e1", Some(&work("internal-admin")), "s");
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert!(!rows[0].billable);
        assert_eq!(reikningshaefi(rows[0].billable), OREIKNINGSHAEFT);
    }

    #[test]
    fn missing_description_falls_back_to_the_event_title() {
        // A calendar title is real work description, so it is eligible —
        // unlike a Claude hook title (see the exclusion test below).
        let c = open_memory().unwrap();
        let b = seed_block(&c, "2026-07-23T09:00:00+00:00", 3600, None, None, false);
        let mut ev = Event::minimal("gcal", "g1", "2026-07-23T09:00:00Z", "Standup");
        ev.project_path = Some("/x/some-folder".to_owned());
        let eid = repository::upsert_event(&c, &ev).unwrap();
        c.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![b, eid],
        )
        .unwrap();
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows[0].invoice_text, "Standup");
    }

    // ───────── regressions found by running against the real database ─────────

    #[test]
    fn claude_hook_titles_never_become_invoice_text() {
        // Regression: on a day that never went through `estimate`, the
        // title fallback surfaced hook names ("PreToolUse", "SessionEnd")
        // as Texti á reikning. Worse, some hook titles carry captured
        // terminal content, which must never reach a customer invoice.
        let c = open_memory().unwrap();
        let b = seed_block(&c, "2026-07-23T09:00:00+00:00", 3600, None, None, false);
        let mut ev = Event::minimal(
            "claude",
            "hook1",
            "2026-07-23T09:00:00Z",
            "UserPromptSubmit — aws dynamodb query --region eu-west-1",
        );
        ev.project_path = Some(work("sjukra"));
        let eid = repository::upsert_event(&c, &ev).unwrap();
        c.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![b, eid],
        )
        .unwrap();

        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1);
        assert!(
            !rows[0].invoice_text.contains("UserPromptSubmit"),
            "hook name leaked into invoice text: {}",
            rows[0].invoice_text
        );
        assert!(
            !rows[0].invoice_text.contains("dynamodb"),
            "captured terminal content leaked into invoice text: {}",
            rows[0].invoice_text
        );
        assert_eq!(rows[0].invoice_text, "Work in sjukra");
    }

    #[test]
    fn a_commit_title_is_still_used_as_invoice_text() {
        // The flip side of the exclusion above: real work titles (commits,
        // PRs, calendar, tickets) remain a good fallback.
        let c = open_memory().unwrap();
        let b = seed_block(&c, "2026-07-23T09:00:00+00:00", 3600, None, None, false);
        let mut ev = Event::minimal(
            "github_commit",
            "c1",
            "2026-07-23T09:00:00Z",
            "Add /practice route — random-question drill mode",
        );
        ev.project_path = Some(work("sjukra"));
        let eid = repository::upsert_event(&c, &ev).unwrap();
        c.execute(
            "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
            params![b, eid],
        )
        .unwrap();

        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(
            rows[0].invoice_text,
            "Add /practice route — random-question drill mode"
        );
    }

    #[test]
    fn unestimated_blocks_collapse_into_one_line_per_folder() {
        // Regression: an un-estimated day (no ticket, no description on any
        // block) emitted one line PER BLOCK — 16 lines of half-hours for a
        // single day. They must collapse per folder instead.
        let c = open_memory().unwrap();
        for (i, start) in [
            "2026-07-23T09:00:00+00:00",
            "2026-07-23T10:00:00+00:00",
            "2026-07-23T11:00:00+00:00",
        ]
        .iter()
        .enumerate()
        {
            let b = seed_block(&c, start, 1800, None, None, false);
            seed_event(&c, b, &format!("e{i}"), Some(&work("sjukra")), "PreToolUse");
        }
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 1, "got {rows:#?}");
        // Three disjoint 30-min blocks → 1.5h union.
        assert_eq!(rows[0].seconds, 5400);
        assert_eq!(rows[0].hours, 1.5);
    }

    #[test]
    fn needs_description_flags_a_fallback_invoice_text() {
        // The user's question: "why do they all say 'Work in sjukra'?"
        // Because the day was never estimated. The flag lets the UI say so
        // instead of looking like the export ignored the descriptions.
        let c = open_memory().unwrap();
        let b = seed_block(&c, "2026-07-23T09:00:00+00:00", 3600, None, None, false);
        seed_event(&c, b, "e1", Some(&work("sjukra")), "PreToolUse");
        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows[0].invoice_text, "Work in sjukra");
        assert!(rows[0].needs_description, "fallback must be flagged");

        // A described block is real text and must NOT be flagged.
        let c2 = open_memory().unwrap();
        let b2 = seed_block(
            &c2,
            "2026-07-23T09:00:00+00:00",
            3600,
            None,
            Some("Implement back button navigation"),
            false,
        );
        seed_event(&c2, b2, "e1", Some(&work("sjukra")), "PreToolUse");
        let rows2 = rows_for_day(&c2, "2026-07-23").unwrap();
        assert_eq!(rows2[0].invoice_text, "Implement back button navigation");
        assert!(!rows2[0].needs_description);
    }

    #[test]
    fn discovery_only_offers_folders_under_the_work_prefix() {
        // Personal paths must not appear as candidate billing folders —
        // ~/dotfiles and ~/Desktop/Projects/* can never be billed, and
        // listing them buries the folders that matter.
        assert_eq!(
            billable_work_folder(&work("autofixer")),
            Some("autofixer".into())
        );
        assert_eq!(
            billable_work_folder(&work("sjukra/.claude/worktrees/x")),
            Some("sjukra".into())
        );
        let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
        assert_eq!(billable_work_folder(&format!("{home}/dotfiles")), None);
        assert_eq!(
            billable_work_folder(&format!("{home}/Desktop/Projects/exam")),
            None
        );
        assert_eq!(billable_work_folder(&work("")), None);
    }

    #[test]
    fn the_work_root_itself_is_not_a_folder() {
        // Regression: a path sitting directly at ~/Desktop/Work produced
        // the nonsense folder "Work", which surfaced as "Work in Work".
        assert_eq!(work_folder_for_path(&work("")), None);
        assert_eq!(
            work_folder_for_path(
                dirs::home_dir()
                    .unwrap()
                    .join("Desktop/Work")
                    .to_str()
                    .unwrap()
            ),
            None
        );
    }

    #[test]
    fn many_descriptions_are_capped_with_a_count_of_the_rest() {
        // Regression: one ticket worked across 20+ blocks joined every
        // description into a paragraph. Cap it and say what was left out.
        let many: Vec<String> = (1..=20)
            .map(|i| format!("Task number {i} with a reasonably long description"))
            .collect();
        let out = join_descriptions(&many);
        assert!(
            out.chars().count() < INVOICE_TEXT_MAX + 40,
            "not capped: {} chars",
            out.chars().count()
        );
        assert!(
            out.contains("more)"),
            "should report the omitted count: {out}"
        );
        assert!(out.starts_with("Task number 1"));
    }

    #[test]
    fn a_single_description_is_used_verbatim() {
        let one = vec!["Build the document analyzer".to_string()];
        assert_eq!(join_descriptions(&one), "Build the document analyzer");
    }

    #[test]
    fn one_very_long_description_is_still_kept() {
        // Never emit an empty invoice line, even if the only description
        // blows the cap on its own.
        let long = vec!["x".repeat(400)];
        assert_eq!(join_descriptions(&long).chars().count(), 400);
    }

    #[test]
    fn rows_that_round_to_zero_hours_are_dropped() {
        // A sub-15-minute group can't be billed, and a wizard full of
        // "0 hrs" lines is noise. Mirrors the old Tempo sync's skip.
        let c = open_memory().unwrap();
        let b = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            300, // 5 minutes
            Some("TINY-1"),
            Some("Blink"),
            false,
        );
        seed_event(&c, b, "e1", Some(&work("sjukra")), "PreToolUse");
        assert!(rows_for_day(&c, "2026-07-23").unwrap().is_empty());
    }

    #[test]
    fn lines_needing_input_sort_first() {
        let c = open_memory().unwrap();
        pin(&c, "apro-website", Some("APRÓ"), Some("Vefsíður"));
        let b1 = seed_block(
            &c,
            "2026-07-23T09:00:00+00:00",
            14400,
            None,
            Some("Resolved"),
            false,
        );
        seed_event(&c, b1, "e1", Some(&work("apro-website")), "s");
        let b2 = seed_block(
            &c,
            "2026-07-23T14:00:00+00:00",
            3600,
            None,
            Some("Unresolved"),
            false,
        );
        seed_event(&c, b2, "e2", Some("/elsewhere/unknown"), "s");

        let rows = rows_for_day(&c, "2026-07-23").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].needs_input(), "needs-input lines come first");
        assert_eq!(rows[0].invoice_text, "Unresolved");
    }

    // ──────────────────────────── rendering ────────────────────────────

    fn sample_rows() -> Vec<BillingRow> {
        vec![
            BillingRow {
                day: "2026-07-23".into(),
                folder: "sjukra".into(),
                customer: Some("Sjúkra".into()),
                verkefni: Some("[P] Vöktun".into()),
                ticket: Some("GENAI-1219".into()),
                seconds: 19800,
                hours: 5.5,
                billable: true,
                invoice_text: "Document analyzer work".into(),
                needs_description: false,
                block_count: 3,
                started_at: "2026-07-23T09:00:00Z".into(),
                ended_at: "2026-07-23T14:30:00Z".into(),
                paths: vec!["/Users/x/Desktop/Work/sjukra".into()],
                block_ids: vec![1, 2, 3],
            },
            BillingRow {
                day: "2026-07-23".into(),
                folder: "genai-infra".into(),
                customer: None,
                verkefni: None,
                ticket: None,
                seconds: 14400,
                hours: 4.0,
                billable: true,
                invoice_text: "Infra work".into(),
                needs_description: false,
                block_count: 1,
                started_at: "2026-07-23T15:00:00Z".into(),
                ended_at: "2026-07-23T19:00:00Z".into(),
                paths: vec!["/Users/x/Desktop/Work/genai-infra".into()],
                block_ids: vec![4],
            },
        ]
    }

    #[test]
    fn date_renders_in_the_forms_dd_mm_yyyy() {
        assert_eq!(sample_rows()[0].date_display(), "23.07.2026");
    }

    #[test]
    fn hours_render_with_a_comma_decimal() {
        assert_eq!(sample_rows()[0].hours_display(), "5,5");
        assert_eq!(sample_rows()[1].hours_display(), "4");
    }

    #[test]
    fn text_shows_blanks_as_a_dash_and_includes_the_constants() {
        let out = render(&sample_rows(), Format::Text);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("23.07.2026"));
        assert!(lines[0].contains("Sjúkra"));
        assert!(lines[0].contains("5,5 hrs"));
        assert!(lines[0].contains(REIKNINGSHAEFT));
        assert!(lines[1].contains(BLANK), "unresolved fields show a dash");
    }

    #[test]
    fn csv_uses_the_form_field_order_and_empty_cells_for_blanks() {
        let out = render(&sample_rows(), Format::Csv);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "dagsetning,vidskiptamadur,verkefni,tegund_skraningar,taxti,timar,reikningshaefi,texti_a_reikning"
        );
        assert!(lines[1].contains("Almenn skráning"));
        assert!(lines[1].contains("Dagvinna"));
        // The comma-decimal hours cell must be quoted.
        assert!(lines[1].contains("\"5,5\""));
        // A blank customer is an empty cell, never a dash.
        assert!(lines[2].starts_with("23.07.2026,,,"), "got {}", lines[2]);
    }

    #[test]
    fn csv_guards_formula_injection_in_the_invoice_text() {
        let mut rows = sample_rows();
        rows[0].invoice_text = "=SUM(A1)".into();
        let out = render(&rows, Format::Csv);
        assert!(out.contains("'=SUM(A1)"), "got: {out}");
    }

    #[test]
    fn json_uses_null_for_unresolved_fields() {
        let out = render(&sample_rows(), Format::Json);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["vidskiptamadur"], "Sjúkra");
        assert_eq!(arr[0]["timar"], 5.5);
        assert_eq!(arr[0]["dagsetning"], "23.07.2026");
        assert_eq!(arr[0]["tegund_skraningar"], "Almenn skráning");
        assert!(arr[1]["vidskiptamadur"].is_null(), "blank must be null");
        assert!(arr[1]["verkefni"].is_null());
    }

    #[test]
    fn all_three_formats_agree_on_the_line_count() {
        let rows = sample_rows();
        let text_lines = render(&rows, Format::Text).lines().count();
        let csv_data = render(&rows, Format::Csv).lines().count() - 1;
        let json_len = serde_json::from_str::<serde_json::Value>(&render(&rows, Format::Json))
            .unwrap()
            .as_array()
            .unwrap()
            .len();
        assert_eq!(text_lines, rows.len());
        assert_eq!(csv_data, rows.len());
        assert_eq!(json_len, rows.len());
    }
}
