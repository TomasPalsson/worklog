//! `worklog ticket` and `worklog account` — the Jira assistant verbs
//! (spec 013). Each talks to the daemon; text by default, `--json` dumps
//! the daemon's body.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Serialize;
use serde_json::json;
use worklog_core::jira_assist_contract::{
    AccountSuggestion, AllowedAccount, AssistCreateBody, AssistCreated, RelearnReport,
    StartOutcome, StartResult, StatusHint, TicketView, FIND_LIMIT,
};
use worklog_core::models::JiraTicket;
use worklog_core::tempo_hub_contract::TicketStatus;

use crate::daemon_client as daemon;
use crate::style;

#[derive(Subcommand, Debug)]
pub enum TicketCmd {
    /// Show a ticket: status, account, description.
    Get { key: String },
    /// Move a GENAI ticket To Do → In Progress (others are left alone).
    Start { key: String },
    /// Free-text Jira search.
    Find { query: String },
    /// Move a GENAI ticket to the named status.
    Move {
        key: String,
        /// Target status name, e.g. "Done".
        to_status: String,
    },
    /// Create a GENAI story with a billing account.
    Create {
        #[arg(long)]
        summary: String,
        /// File holding the markdown description.
        #[arg(long)]
        description_file: PathBuf,
        /// Account id (see `worklog account allowed`).
        #[arg(long)]
        account: String,
        /// worklog's first suggestion, when one was shown; differs from
        /// `--account` when the Owner corrected it.
        #[arg(long)]
        guessed: Option<String>,
        /// Clue from the request text; repeatable.
        #[arg(long = "clue")]
        clues: Vec<String>,
    },
    /// Suggested status moves (merged PRs).
    Hints,
    /// Record the Jira ticket this Claude Code session is on.
    Use {
        key: String,
        #[arg(long)]
        session: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum AccountCmd {
    /// Accounts GENAI stories may use.
    Allowed,
    /// Rank accounts for a piece of text.
    Suggest { text: String },
    /// Relearn account clues from existing tickets.
    Relearn,
}

fn dump<W: Write, T: Serialize>(out: &mut W, v: &T) -> Result<()> {
    writeln!(out, "{}", serde_json::to_string_pretty(v)?)?;
    Ok(())
}

pub fn run_ticket<W: Write>(sub: TicketCmd, out: &mut W, json: bool) -> Result<()> {
    match sub {
        TicketCmd::Get { key } => {
            let v: TicketView = daemon::get(&format!("/tickets/{key}/view"))?;
            if json {
                return dump(out, &v);
            }
            render_view(out, &v)
        }
        TicketCmd::Start { key } => {
            let r: StartResult = daemon::post(&format!("/tickets/{key}/start"), &json!({}))?;
            if json {
                return dump(out, &r);
            }
            render_start(out, &r)
        }
        TicketCmd::Find { query } => {
            let url = reqwest::Url::parse_with_params(
                "http://x/tickets/search",
                [("q", query.as_str()), ("limit", &FIND_LIMIT.to_string())],
            )?;
            let hits: Vec<JiraTicket> =
                daemon::get(&format!("{}?{}", url.path(), url.query().unwrap_or("")))?;
            if json {
                return dump(out, &hits);
            }
            render_find(out, &hits)
        }
        TicketCmd::Move { key, to_status } => {
            let s: TicketStatus = daemon::post(
                &format!("/tickets/{key}/move"),
                &json!({ "to_status": to_status }),
            )?;
            if json {
                return dump(out, &s);
            }
            Ok(style::ok(out, &format!("{} is now {}", s.key, s.status))?)
        }
        TicketCmd::Create {
            summary,
            description_file,
            account,
            guessed,
            clues,
        } => create(
            summary,
            &description_file,
            account,
            guessed,
            clues,
            out,
            json,
        ),
        TicketCmd::Use { key, session } => {
            let paths = worklog_core::paths::Paths::resolve()?;
            let conn = worklog_core::db::open(&paths.db)?;
            worklog_core::session_tickets::set(&conn, &session, &key)?;
            if json {
                return dump(out, &json!({ "session_id": session, "jira_issue": key }));
            }
            let short: String = session.chars().take(8).collect();
            Ok(writeln!(out, "Session {short} is on {key}")?)
        }
        TicketCmd::Hints => {
            let h: Vec<StatusHint> = daemon::get("/hints")?;
            if json {
                return dump(out, &h);
            }
            render_hints(out, &h)
        }
    }
}

pub fn run_account<W: Write>(sub: AccountCmd, out: &mut W, json: bool) -> Result<()> {
    match sub {
        AccountCmd::Allowed => {
            let a: Vec<AllowedAccount> = daemon::get("/accounts/allowed")?;
            if json {
                return dump(out, &a);
            }
            for x in &a {
                writeln!(out, "{}  {}", x.id, x.name)?;
            }
            Ok(())
        }
        AccountCmd::Suggest { text } => {
            let s: Vec<AccountSuggestion> =
                daemon::post("/accounts/suggest", &json!({ "text": text }))?;
            if json {
                return dump(out, &s);
            }
            render_suggestions(out, &s)
        }
        AccountCmd::Relearn => {
            let r: RelearnReport = daemon::post("/accounts/relearn", &json!({}))?;
            if json {
                return dump(out, &r);
            }
            style::ok(
                out,
                &format!(
                    "read {} tickets: {} accounts, {} clues",
                    r.tickets_read, r.accounts, r.clues
                ),
            )?;
            Ok(())
        }
    }
}

fn create<W: Write>(
    summary: String,
    description_file: &std::path::Path,
    account: String,
    guessed: Option<String>,
    clues: Vec<String>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let description = std::fs::read_to_string(description_file)
        .with_context(|| format!("reading {}", description_file.display()))?;
    let body = create_body(summary, description, account, guessed, clues);
    let c: AssistCreated = daemon::post("/tickets/assist-create", &serde_json::to_value(&body)?)?;
    if json {
        return dump(out, &c);
    }
    style::ok(out, &format!("created {} ({})", c.key, c.account.name))?;
    writeln!(out, "{}", c.url)?;
    Ok(())
}

fn create_body(
    summary: String,
    description: String,
    account: String,
    guessed: Option<String>,
    clues: Vec<String>,
) -> AssistCreateBody {
    AssistCreateBody {
        summary,
        description,
        guessed_account_id: guessed,
        account_id: account,
        clues,
        assignee_account_id: None,
    }
}

fn render_view<W: Write>(out: &mut W, v: &TicketView) -> Result<()> {
    let d = &v.detail;
    writeln!(out, "{}  {}", d.key, d.summary)?;
    writeln!(out, "status:  {}", d.status.as_deref().unwrap_or("-"))?;
    writeln!(out, "account: {}", v.account_name.as_deref().unwrap_or("-"))?;
    writeln!(out, "url:     {}", d.url)?;
    if !d.description.is_empty() {
        writeln!(out, "\n{}", d.description)?;
    }
    Ok(())
}

fn render_start<W: Write>(out: &mut W, r: &StartResult) -> Result<()> {
    let key = &r.view.detail.key;
    let status = r.view.detail.status.as_deref().unwrap_or("-");
    let msg = match r.outcome {
        StartOutcome::Moved => format!("{key} started ({status})"),
        StartOutcome::AlreadyStarted => format!("{key} already started ({status})"),
        StartOutcome::NotWritable => format!("{key} is not a GENAI ticket; left alone ({status})"),
    };
    style::ok(out, &msg)?;
    Ok(())
}

fn render_find<W: Write>(out: &mut W, hits: &[JiraTicket]) -> Result<()> {
    if hits.is_empty() {
        style::warn(out, "no matching tickets")?;
    }
    for t in hits {
        writeln!(
            out,
            "{}  [{}]  {}",
            t.key,
            t.status.as_deref().unwrap_or("-"),
            t.summary
        )?;
    }
    Ok(())
}

fn render_suggestions<W: Write>(out: &mut W, s: &[AccountSuggestion]) -> Result<()> {
    if s.is_empty() {
        style::warn(out, "no account suggestions")?;
    }
    for x in s {
        writeln!(
            out,
            "{}  {}  score {:.2}  ({} past tickets; clues: {})",
            x.account.id,
            x.account.name,
            x.score,
            x.past_tickets,
            x.matched_clues.join(", ")
        )?;
    }
    Ok(())
}

fn render_hints<W: Write>(out: &mut W, hints: &[StatusHint]) -> Result<()> {
    if hints.is_empty() {
        style::info(out, "no status hints")?;
    }
    for h in hints {
        writeln!(
            out,
            "{}  {}  -> {}",
            h.key,
            h.summary,
            h.to_category.as_str()
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "ticket_cmd_test.rs"]
mod tests;
