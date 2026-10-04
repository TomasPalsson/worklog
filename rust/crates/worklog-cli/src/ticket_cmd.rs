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
        /// The account was worklog's first suggestion, not the Owner's pick.
        #[arg(long)]
        guess: bool,
        /// Clue from the request text; repeatable.
        #[arg(long = "clue")]
        clues: Vec<String>,
    },
    /// Suggested status moves (merged PRs).
    Hints,
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
            style::ok(out, &format!("{} is now {}", s.key, s.status))?;
            Ok(())
        }
        TicketCmd::Create {
            summary,
            description_file,
            account,
            guess,
            clues,
        } => create(summary, &description_file, account, guess, clues, out, json),
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
    guess: bool,
    clues: Vec<String>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let description = std::fs::read_to_string(description_file)
        .with_context(|| format!("reading {}", description_file.display()))?;
    let body = create_body(summary, description, account, guess, clues);
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
    guess: bool,
    clues: Vec<String>,
) -> AssistCreateBody {
    AssistCreateBody {
        summary,
        description,
        guessed_account_id: guess.then(|| account.clone()),
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
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;
    use worklog_core::jira_assist_contract::HintReason;
    use worklog_core::tempo_hub_contract::{StatusCategory, TicketDetail};

    fn detail(status: &str) -> TicketDetail {
        serde_json::from_value(json!({
            "key": "GENAI-7", "summary": "Do thing", "status": status,
            "status_category": null, "issue_type": null, "priority": null,
            "assignee": null, "updated": null, "url": "https://j/browse/GENAI-7",
            "description": "body text", "comments": []
        }))
        .unwrap()
    }

    fn render<F: FnOnce(&mut Vec<u8>) -> Result<()>>(f: F) -> String {
        let mut buf = Vec::new();
        f(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn every_verb_parses() {
        for args in [
            "ticket get GENAI-1",
            "ticket start GENAI-1",
            "ticket find some words",
            "ticket move GENAI-1 Done",
            "ticket hints",
            "account allowed",
            "account suggest hello",
            "account relearn",
        ] {
            let mut argv = vec!["worklog"];
            argv.extend(args.split(' '));
            // `find some words` takes one quoted arg in real use.
            if args.starts_with("ticket find") {
                argv = vec!["worklog", "ticket", "find", "some words"];
            }
            Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{args}: {e}"));
        }
    }

    #[test]
    fn create_parses_repeated_clues_and_guess() {
        let cli = Cli::try_parse_from([
            "worklog",
            "ticket",
            "create",
            "--summary",
            "S",
            "--description-file",
            "d.md",
            "--account",
            "42",
            "--guess",
            "--clue",
            "a",
            "--clue",
            "b",
        ])
        .unwrap();
        let crate::cli::Cmd::Ticket {
            sub:
                TicketCmd::Create {
                    guess,
                    clues,
                    account,
                    ..
                },
        } = cli.command
        else {
            panic!("not ticket create");
        };
        assert!(guess);
        assert_eq!(account, "42");
        assert_eq!(clues, ["a", "b"]);
    }

    #[test]
    fn guess_flag_sets_guessed_account() {
        let b = create_body("S".into(), "D".into(), "42".into(), true, vec!["x".into()]);
        assert_eq!(b.guessed_account_id.as_deref(), Some("42"));
        assert_eq!(b.account_id, "42");
        assert_eq!(b.clues, ["x"]);
        let b = create_body("S".into(), "D".into(), "42".into(), false, vec![]);
        assert_eq!(b.guessed_account_id, None);
    }

    #[test]
    fn view_text_shows_key_status_account_description() {
        let v = TicketView {
            detail: detail("To Do"),
            account_id: Some("9".into()),
            account_name: Some("Acme".into()),
        };
        let s = render(|o| render_view(o, &v));
        assert!(s.contains("GENAI-7  Do thing"), "{s}");
        assert!(s.contains("status:  To Do"), "{s}");
        assert!(s.contains("account: Acme"), "{s}");
        assert!(s.contains("body text"), "{s}");
    }

    #[test]
    fn start_text_names_each_outcome() {
        let mk = |outcome| StartResult {
            view: TicketView {
                detail: detail("In Progress"),
                account_id: None,
                account_name: None,
            },
            outcome,
        };
        assert!(render(|o| render_start(o, &mk(StartOutcome::Moved))).contains("GENAI-7 started"));
        assert!(
            render(|o| render_start(o, &mk(StartOutcome::AlreadyStarted)))
                .contains("already started")
        );
        assert!(render(|o| render_start(o, &mk(StartOutcome::NotWritable)))
            .contains("not a GENAI ticket"));
    }

    #[test]
    fn find_and_suggestions_and_hints_render_rows() {
        let hit = JiraTicket {
            key: "ABC-1".into(),
            summary: "Found".into(),
            status: Some("Done".into()),
            project_key: None,
            updated: None,
            issue_id: None,
        };
        assert_eq!(
            render(|o| render_find(o, &[hit])).trim(),
            "ABC-1  [Done]  Found"
        );
        let sug = AccountSuggestion {
            account: AllowedAccount {
                id: "9".into(),
                name: "Acme".into(),
            },
            matched_clues: vec!["x".into(), "y".into()],
            past_tickets: 3,
            score: 1.5,
        };
        assert_eq!(
            render(|o| render_suggestions(o, &[sug])).trim(),
            "9  Acme  score 1.50  (3 past tickets; clues: x, y)"
        );
        let hint = StatusHint {
            key: "GENAI-7".into(),
            summary: "Do thing".into(),
            to_category: StatusCategory::Done,
            reason: HintReason::PrMerged {
                repo: "r".into(),
                number: 1,
                merged_at: "t".into(),
            },
        };
        let s = render(|o| render_hints(o, &[hint]));
        assert!(s.starts_with("GENAI-7  Do thing  -> "), "{s}");
    }
}
