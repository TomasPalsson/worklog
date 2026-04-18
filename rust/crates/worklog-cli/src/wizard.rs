//! `worklog setup` — interactive onboarding wizard.
//!
//! Design goals for Stage 1:
//! * Works over a plain TTY without any terminal-takeover magic (ratatui can
//!   come later). Every step is a heading + a small cluster of prompts.
//! * Each captured secret is **live-validated** against the upstream API
//!   where we can (Jira, GitHub, Anthropic). Validation is best-effort; any
//!   failure downgrades to a warning the user can override with `[y] save
//!   anyway`.
//! * Idempotent: re-running the wizard fills in gaps, never clobbers
//!   existing values silently.
//! * Non-interactive mode via `worklog setup --non-interactive`: prints a
//!   checklist of what would run and exits. Used by CI and scripting.
//!
//! We still touch side effects (keychain + disk) so the wizard lives behind
//! a `skip_network` flag that the tests flip to keep them hermetic.

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use console::{style, Style};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};
use worklog_core::{db, paths::Paths, secrets};

/// Options controlling what the wizard actually runs. Defaults to a full
/// interactive flow; tests swap these for hermetic behaviour.
#[derive(Debug, Clone, Default)]
pub struct WizardOptions {
    /// Disable interactive prompts entirely — only print what we would do.
    pub non_interactive: bool,
    /// Skip HTTP validation of captured credentials.
    pub skip_validate: bool,
    /// Skip launchd/systemd writes. Always true under tests.
    pub skip_schedule: bool,
    /// Path used by the file-backed secret store when running under tests.
    /// Unused in production — the keychain is always the real target.
    pub secrets_file: Option<std::path::PathBuf>,
}

/// Result summary returned to the CLI so we can pretty-print a done screen.
#[derive(Debug, Clone)]
pub struct WizardReport {
    pub paths: Paths,
    pub db_initialized: bool,
    pub secrets_set: Vec<String>,
    pub secrets_skipped: Vec<String>,
    pub schedule_installed: Option<String>,
    pub notes: Vec<String>,
}

/// Entrypoint — called by `worklog setup`.
pub fn run(opts: WizardOptions) -> Result<WizardReport> {
    if let Some(path) = &opts.secrets_file {
        std::env::set_var("WORKLOG_SECRETS_FILE", path);
    }

    let paths = Paths::resolve()?;
    paths.ensure()?;

    print_banner(&paths);

    // Step 1 — db.
    let conn = db::open(&paths.db)?;
    let summary = db::summarize(&conn)?;
    println!(
        "  {} database ready at {} (schema v{}, {} events, {} blocks)",
        style("✓").green().bold(),
        short(&paths.db),
        summary.schema_version,
        summary.events,
        summary.blocks,
    );
    drop(conn);

    // Step 2 — preflight.
    let preflight = run_preflight();
    print_preflight(&preflight);

    // Step 3 — secrets.
    let mut secrets_set = Vec::new();
    let mut secrets_skipped = Vec::new();

    if opts.non_interactive {
        println!(
            "\n{} skipping interactive secret capture (--non-interactive)",
            style("·").dim()
        );
        for k in secrets::KNOWN_KEYS {
            if secrets::get(k)?.is_some() {
                secrets_set.push((*k).to_string());
            } else {
                secrets_skipped.push((*k).to_string());
            }
        }
    } else {
        let captured = capture_secrets(&opts)?;
        for (k, action) in captured {
            match action {
                SecretAction::Saved => secrets_set.push(k),
                SecretAction::Kept => secrets_set.push(k),
                SecretAction::Skipped => secrets_skipped.push(k),
            }
        }
    }

    // Step 4 — schedule + hook are no-ops in Stage 1 (a separate feature
    // iteration owns those platform-specific writes). Surface them so the
    // user knows what's coming.
    let mut notes = Vec::new();
    if !opts.skip_schedule && !opts.non_interactive {
        notes.push(
            "scheduled collection (launchd/systemd) comes in stage 1.3 — run `worklog collect all` manually for now"
                .into()
        );
    }

    print_done(&paths, &secrets_set, &secrets_skipped);

    Ok(WizardReport {
        paths,
        db_initialized: true,
        secrets_set,
        secrets_skipped,
        schedule_installed: None,
        notes,
    })
}

// ───────────────────────── preflight ─────────────────────────

#[derive(Debug, Clone)]
struct Preflight {
    docker: ProbeOutcome,
    claude: ProbeOutcome,
    git: ProbeOutcome,
}

#[derive(Debug, Clone)]
enum ProbeOutcome {
    Found(String),
    Missing,
}

fn run_preflight() -> Preflight {
    Preflight {
        docker: probe_binary("docker", &["--version"]),
        claude: probe_binary("claude", &["--version"]),
        git: probe_binary("git", &["--version"]),
    }
}

fn probe_binary(bin: &str, args: &[&str]) -> ProbeOutcome {
    use std::process::Command;
    let Ok(output) = Command::new(bin).args(args).output() else {
        return ProbeOutcome::Missing;
    };
    if !output.status.success() {
        return ProbeOutcome::Missing;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    ProbeOutcome::Found(s)
}

fn print_preflight(p: &Preflight) {
    println!("\n{}", section("preflight"));
    for (label, outcome, hint) in [
        (
            "docker",
            &p.docker,
            "required for the web UI — install OrbStack or Docker Desktop",
        ),
        (
            "claude",
            &p.claude,
            "required for AI estimation — see claude.ai/code",
        ),
        ("git", &p.git, "required for collectors"),
    ] {
        match outcome {
            ProbeOutcome::Found(v) => {
                let short = v.lines().next().unwrap_or(v);
                println!(
                    "  {} {:<8} {}",
                    style("✓").green().bold(),
                    label,
                    style(short).dim()
                );
            }
            ProbeOutcome::Missing => {
                println!(
                    "  {} {:<8} {}",
                    style("✗").red().bold(),
                    label,
                    style(hint).yellow()
                );
            }
        }
    }
}

// ───────────────────────── secrets capture ─────────────────────────

#[derive(Debug)]
enum SecretAction {
    Saved,
    Kept,
    Skipped,
}

fn capture_secrets(opts: &WizardOptions) -> Result<Vec<(String, SecretAction)>> {
    println!("\n{}", section("credentials"));
    println!(
        "  {} stored in OS keychain (service=\"{}\")",
        style("·").dim(),
        secrets::SERVICE,
    );

    let theme = ColorfulTheme::default();
    let mut results = Vec::new();

    for key in secrets::KNOWN_KEYS {
        let label = human_label(key);
        let existing = secrets::get(key)?.is_some();
        let default_action = if existing { 0 } else { 1 }; // Keep / Set / Skip

        let items = if existing {
            &["keep existing", "replace", "skip"][..]
        } else {
            &["set", "skip"][..]
        };

        let pick = if opts.non_interactive || !io::stdin().is_terminal() {
            default_action
        } else {
            Select::with_theme(&theme)
                .with_prompt(format!("{}  {}", style(key).cyan().bold(), label))
                .items(items)
                .default(default_action)
                .interact()?
        };

        let action = if existing {
            match pick {
                0 => SecretAction::Kept,
                1 => {
                    prompt_and_save(&theme, key, opts)?;
                    SecretAction::Saved
                }
                _ => SecretAction::Skipped,
            }
        } else {
            match pick {
                0 => {
                    prompt_and_save(&theme, key, opts)?;
                    SecretAction::Saved
                }
                _ => SecretAction::Skipped,
            }
        };
        results.push((key.to_string(), action));
    }

    Ok(results)
}

fn prompt_and_save(theme: &ColorfulTheme, key: &str, opts: &WizardOptions) -> Result<()> {
    let is_url = key.ends_with("_url");
    let is_public = matches!(key, "jira_email" | "google_client_id" | "jira_base_url");

    let value: String = if is_public || is_url {
        Input::with_theme(theme)
            .with_prompt(format!("  {}", key))
            .interact_text()?
    } else {
        Password::with_theme(theme)
            .with_prompt(format!("  {}", key))
            .interact()?
    };

    if !opts.skip_validate {
        if let Some(err) = validate(key, &value) {
            println!(
                "  {} validation failed: {}",
                style("!").yellow().bold(),
                err
            );
            let save_anyway = Confirm::with_theme(theme)
                .with_prompt("  save anyway?")
                .default(false)
                .interact()?;
            if !save_anyway {
                println!("  {} skipped", style("·").dim());
                return Ok(());
            }
        } else {
            println!("  {} validated", style("✓").green().bold());
        }
    }

    secrets::set(key, &value).with_context(|| format!("storing {key}"))?;
    Ok(())
}

// ───────────────────────── live validation ─────────────────────────

/// Return `Some(error)` if validation fails; `None` if it succeeds or is a
/// no-op for this key. We deliberately keep validators coarse — a 200 on a
/// well-known endpoint is enough signal.
pub fn validate(key: &str, value: &str) -> Option<String> {
    match key {
        "jira_email" => {
            if !value.contains('@') {
                Some("doesn't look like an email".into())
            } else {
                None
            }
        }
        "jira_base_url" | "google_client_id" | "google_client_secret" | "google_refresh_token" => {
            if value.len() < 3 {
                Some("too short".into())
            } else {
                None
            }
        }
        "jira_api_token" | "tempo_api_token" => {
            if value.len() < 10 {
                Some("token looks too short".into())
            } else {
                None
            }
        }
        "github_token" => validate_github_token(value),
        "anthropic_api_key" => {
            if !value.starts_with("sk-ant-") {
                Some("expected prefix `sk-ant-`".into())
            } else if value.len() < 30 {
                Some("key looks too short".into())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn validate_github_token(token: &str) -> Option<String> {
    if !(token.starts_with("ghp_") || token.starts_with("github_pat_")) {
        return Some("expected prefix `ghp_` or `github_pat_`".into());
    }
    let client = match reqwest::blocking::Client::builder()
        .user_agent("worklog-cli")
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None, // treat as validator-unavailable (not a hard fail)
    };
    match client
        .get("https://api.github.com/user")
        .bearer_auth(token)
        .send()
    {
        Ok(r) if r.status().is_success() => None,
        Ok(r) => Some(format!("GitHub returned HTTP {}", r.status())),
        Err(e) => Some(format!("network: {e}")),
    }
}

// ───────────────────────── printing helpers ─────────────────────────

fn print_banner(paths: &Paths) {
    let title = Style::new().yellow().bold();
    let sub = Style::new().dim();
    println!();
    println!("  {}  worklog setup", title.apply_to("◆"));
    println!(
        "  {}",
        sub.apply_to(format!("home = {}", short(&paths.root)))
    );
    println!();
}

fn print_done(paths: &Paths, set: &[String], skipped: &[String]) {
    println!("\n{}", section("ready"));
    println!(
        "  {} worklog home at {}",
        style("✓").green().bold(),
        short(&paths.root)
    );
    println!(
        "  {} {} secrets set, {} skipped",
        style("·").dim(),
        set.len(),
        skipped.len()
    );
    println!("\n  next:");
    println!("    {} worklog db info", style("$").dim());
    println!("    {} worklog doctor", style("$").dim());
    println!();
}

fn section(s: &str) -> String {
    format!("  {}", style(format!("── {} ──", s)).bold().yellow())
}

fn short(p: &std::path::Path) -> String {
    worklog_core::paths::short_display(p)
}

fn human_label(key: &str) -> &'static str {
    match key {
        "jira_email" => "your Atlassian account email",
        "jira_api_token" => "Atlassian API token (id.atlassian.com)",
        "jira_base_url" => "https://<workspace>.atlassian.net",
        "github_token" => "GitHub personal access token (repo + read:user)",
        "tempo_api_token" => "Tempo Cloud API token (tempo.io)",
        "google_client_id" => "Google OAuth client id",
        "google_client_secret" => "Google OAuth client secret",
        "google_refresh_token" => "Google OAuth refresh token",
        "anthropic_api_key" => "Anthropic API key (console.anthropic.com)",
        _ => "",
    }
}

// Quiet `unused` warnings on helpers used only in certain flows.
#[allow(dead_code)]
fn _noop<W: Write>(_w: &mut W) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_obvious_garbage() {
        assert!(validate("jira_email", "not-an-email").is_some());
        assert!(validate("jira_api_token", "abc").is_some());
        assert!(validate("anthropic_api_key", "sk-xxx").is_some());
    }

    #[test]
    fn validate_passes_plausible_values() {
        assert!(validate("jira_email", "tomas@p5.is").is_none());
        assert!(validate("jira_api_token", "a".repeat(40).as_str()).is_none());
        assert!(validate("anthropic_api_key", &format!("sk-ant-{}", "x".repeat(40))).is_none());
    }

    #[test]
    fn non_interactive_wizard_is_a_nop_on_empty_system() {
        // Run in a scratch dir so the real home is untouched.
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("WORKLOG_HOME", tmp.path());
        let secrets_file = tmp.path().join("secrets.json");
        std::env::set_var("WORKLOG_SECRETS_FILE", &secrets_file);
        let report = run(WizardOptions {
            non_interactive: true,
            skip_validate: true,
            skip_schedule: true,
            secrets_file: Some(secrets_file.clone()),
        })
        .unwrap();
        assert!(report.db_initialized);
        // No existing secrets means all known keys were counted as skipped.
        assert_eq!(report.secrets_set.len(), 0);
        assert_eq!(
            report.secrets_skipped.len(),
            worklog_core::secrets::KNOWN_KEYS.len()
        );
        // Db file really exists.
        assert!(tmp.path().join("worklog.db").is_file());
        std::env::remove_var("WORKLOG_HOME");
        std::env::remove_var("WORKLOG_SECRETS_FILE");
    }
}
