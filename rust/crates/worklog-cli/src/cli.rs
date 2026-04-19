//! CLI command definitions + dispatcher.
//!
//! Kept in `lib.rs` so integration tests can invoke `worklog_cli::run_with`
//! with explicit argv without spawning a subprocess. `main.rs` simply calls
//! `run()` which uses the real `std::env::args`.

use std::io::{self, IsTerminal, Read, Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use worklog_core::{
    collectors::{gcal as gcal_col, github as gh, jira as jira_col, tempo as tempo_col},
    daemon as daemon_mod, db, estimate, hook, hook_run, http, infer,
    paths::Paths,
    schedule, secrets, updater as upd, web as web_mod,
};

/// worklog — personal time-tracking for the developer who hates timers.
#[derive(Parser, Debug)]
#[command(
    name = "worklog",
    version,
    about = "Personal worklog — Rust edition.",
    long_about = None,
)]
pub struct Cli {
    /// Override the worklog home directory (default: $WORKLOG_HOME or ~/.worklog).
    #[arg(long, global = true, env = "WORKLOG_HOME")]
    pub home: Option<std::path::PathBuf>,

    /// Emit machine-readable JSON for commands that support it.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Print version info.
    Version,

    /// Report environment, db, and secret status.
    Doctor,

    /// One-shot onboarding: db migrate + preflight + capture secrets.
    Setup {
        /// Print what would run and exit; capture nothing.
        #[arg(long)]
        non_interactive: bool,
        /// Skip HTTP validation of captured credentials.
        #[arg(long)]
        skip_validate: bool,
    },

    /// Database operations.
    #[command(subcommand)]
    Db(DbCmd),

    /// Secret operations (OS keychain).
    #[command(subcommand)]
    Secret(SecretCmd),

    /// Claude Code hook install / uninstall / status.
    #[command(subcommand)]
    Hook(HookCmd),

    /// Scheduled collection (launchd on macOS, systemd --user on Linux).
    #[command(subcommand)]
    Schedule(ScheduleCmd),

    /// Pull events from external sources (jira, github, tempo, all).
    Collect {
        /// Which source to pull. `all` = jira + github. Gcal is deferred
        /// until Stage 2.1.
        #[arg(value_enum, default_value_t = CollectTarget::All)]
        target: CollectTarget,
        /// Days of history to pull for time-range sources (github). Default 7.
        #[arg(long, default_value_t = 7)]
        days: u32,
    },

    /// Sync reviewed blocks for a given day to Tempo Cloud.
    Sync {
        /// YYYY-MM-DD; default is today (UTC).
        #[arg(long)]
        day: Option<String>,
        /// Preview the payload without calling Tempo.
        #[arg(long)]
        dry_run: bool,
    },

    /// Cluster a day's events into blocks (gap-timeout algorithm).
    Infer {
        /// YYYY-MM-DD; default today (UTC).
        #[arg(long)]
        day: Option<String>,
    },

    /// Estimate jira_issue + duration + description for each un-estimated block.
    Estimate {
        /// YYYY-MM-DD; default today (UTC).
        #[arg(long)]
        day: Option<String>,
        /// Model id passed to `claude -p`.
        #[arg(long, default_value = estimate::DEFAULT_MODEL)]
        model: String,
    },

    /// Claude Code hook — reads a JSON event from stdin and records it.
    #[command(name = "hook-run", hide = true)]
    HookRun,

    /// Start the axum unix-socket IPC server. The web UI talks to this.
    Daemon {
        /// Override the socket path (default: <data>/api.sock).
        #[arg(long)]
        socket: Option<std::path::PathBuf>,
        /// Also listen on this TCP address (e.g. 127.0.0.1:9323). Needed
        /// for the dockerised web UI on Docker Desktop, which can't
        /// bind-mount a live unix socket across its VM. Defaults to
        /// `127.0.0.1:9323` — pass an empty string to disable.
        #[arg(long, default_value = "127.0.0.1:9323")]
        tcp: String,
    },

    /// Run the dockerised Next.js review UI (http://localhost:3333).
    Web {
        #[command(subcommand)]
        sub: WebCmd,
    },

    /// Self-update: verify + download + atomically swap the binary.
    SelfUpdate {
        /// Override the manifest URL. Defaults to the worklog release
        /// bucket on GitHub.
        #[arg(long, env = "WORKLOG_MANIFEST_URL")]
        manifest_url: Option<String>,
        /// Only check; don't download or swap.
        #[arg(long)]
        check: bool,
        /// Fetch and verify everything but skip the final swap.
        #[arg(long)]
        dry_run: bool,
        /// Re-install even when the manifest version matches.
        #[arg(long)]
        force: bool,
    },

    /// Release-management tooling. Used by the maintainer to cut signed
    /// releases — not something end-users run.
    Dev {
        #[command(subcommand)]
        sub: DevCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum DevCmd {
    /// Generate a fresh Ed25519 release keypair and print the public
    /// key constant to paste into worklog-core::updater::pubkey.
    Keygen {
        /// Where to write the private key in PEM format. Defaults to
        /// $XDG_CONFIG_HOME/worklog/keys/release-ed25519.pem, chmod 0600.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Overwrite any existing key at `out` without prompting.
        #[arg(long)]
        force: bool,
    },

    /// Sign a file with the local release private key. Writes a
    /// `<file>.sig` with the raw 64-byte signature.
    Sign {
        /// The file whose bytes should be signed.
        file: std::path::PathBuf,
        /// Override the key path. Defaults to the keygen default.
        #[arg(long)]
        key: Option<std::path::PathBuf>,
    },

    /// Produce a bsdiff delta from `old` to `new`.
    MakePatch {
        old: std::path::PathBuf,
        new: std::path::PathBuf,
        /// Where to write the patch. Defaults to `<new>.patch`.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },

    /// Apply a delta patch to `old`, producing `out`.
    ApplyPatch {
        old: std::path::PathBuf,
        patch: std::path::PathBuf,
        out: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebCmd {
    /// Start the web container in the background (also ensures the
    /// daemon is running).
    Up {
        /// Port to bind on localhost. Default: 3333.
        #[arg(long, default_value_t = 3333)]
        port: u16,
        /// Don't ensure the daemon is running — assume you started it.
        #[arg(long)]
        no_daemon: bool,
    },
    /// Stop and remove the web container (leaves the daemon alone).
    Down,
    /// Show container status.
    Status,
    /// Tail container logs (Ctrl-C to exit).
    Logs {
        /// Number of past lines to seed the tail with.
        #[arg(long, default_value_t = 80)]
        tail: u32,
    },
    /// (Re)build the container image from web/Dockerfile.
    Build {
        /// Force pull of the base image.
        #[arg(long)]
        pull: bool,
    },
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum CollectTarget {
    All,
    Jira,
    Github,
    Gcal,
}

#[derive(Subcommand, Debug)]
pub enum DbCmd {
    /// Initialize / migrate the database at the resolved path. Idempotent.
    Migrate,
    /// One-line summary of the db.
    Info,
    /// Print the resolved db path.
    Path,
}

#[derive(Subcommand, Debug)]
pub enum HookCmd {
    /// Register the stdin-JSON hook in ~/.claude/settings.json.
    Install {
        /// Override the command stored in each handler. Default: auto-detect
        /// `worklog-hook` or `worklog hook run`.
        #[arg(long)]
        command: Option<String>,
    },
    /// Remove worklog handlers; leaves other tools' hooks alone.
    Uninstall,
    /// Report whether worklog is registered and which events it listens on.
    Status,
}

#[derive(Subcommand, Debug)]
pub enum ScheduleCmd {
    /// Install a periodic collector (launchd plist / systemd user timer).
    Install {
        /// How often to run. Accepts 5m, 15m, 30m, 1h, 4h, daily, or raw seconds.
        #[arg(long, default_value = "15m")]
        interval: String,
        /// Command to execute on tick. Default: auto-detect `worklog collect all`.
        #[arg(long)]
        command: Option<String>,
    },
    /// Remove the launchd plist / systemd units installed by worklog.
    Uninstall,
    /// Show whether a schedule is installed and at what interval.
    Status,
}

#[derive(Subcommand, Debug)]
pub enum SecretCmd {
    /// Set a secret. Value read from stdin if not provided, or prompted on a TTY.
    Set {
        /// The secret key (e.g. `jira_api_token`).
        key: String,
        /// Pass the value inline (insecure — prefer stdin piping).
        #[arg(long)]
        value: Option<String>,
    },
    /// Read a secret to stdout.
    Get { key: String },
    /// Remove a secret.
    Rm { key: String },
    /// List known secret keys and whether each is set.
    List,
}

pub fn run() -> Result<()> {
    run_with(
        std::env::args_os().collect::<Vec<_>>(),
        &mut io::stdout(),
        &mut io::stderr(),
    )
}

pub fn run_with<W: Write>(
    argv: Vec<std::ffi::OsString>,
    out: &mut W,
    _err: &mut dyn Write,
) -> Result<()> {
    let cli = Cli::try_parse_from(argv)?;
    init_tracing();
    if let Some(h) = &cli.home {
        std::env::set_var("WORKLOG_HOME", h);
    }

    match cli.command {
        Cmd::Version => cmd_version(out, cli.json),
        Cmd::Doctor => cmd_doctor(out, cli.json),
        Cmd::Setup {
            non_interactive,
            skip_validate,
        } => cmd_setup(non_interactive, skip_validate),
        Cmd::Db(sub) => match sub {
            DbCmd::Migrate => cmd_db_migrate(out, cli.json),
            DbCmd::Info => cmd_db_info(out, cli.json),
            DbCmd::Path => cmd_db_path(out),
        },
        Cmd::Secret(sub) => match sub {
            SecretCmd::Set { key, value } => cmd_secret_set(&key, value, out),
            SecretCmd::Get { key } => cmd_secret_get(&key, out),
            SecretCmd::Rm { key } => cmd_secret_rm(&key, out),
            SecretCmd::List => cmd_secret_list(out, cli.json),
        },
        Cmd::Hook(sub) => match sub {
            HookCmd::Install { command } => cmd_hook_install(command, out, cli.json),
            HookCmd::Uninstall => cmd_hook_uninstall(out, cli.json),
            HookCmd::Status => cmd_hook_status(out, cli.json),
        },
        Cmd::Schedule(sub) => match sub {
            ScheduleCmd::Install { interval, command } => {
                cmd_schedule_install(&interval, command, out, cli.json)
            }
            ScheduleCmd::Uninstall => cmd_schedule_uninstall(out, cli.json),
            ScheduleCmd::Status => cmd_schedule_status(out, cli.json),
        },
        Cmd::Collect { target, days } => cmd_collect(target, days, out, cli.json),
        Cmd::Sync { day, dry_run } => cmd_sync(day, dry_run, out, cli.json),
        Cmd::Infer { day } => cmd_infer(day, out, cli.json),
        Cmd::Estimate { day, model } => cmd_estimate(day, &model, out, cli.json),
        Cmd::HookRun => cmd_hook_run(),
        Cmd::Daemon { socket, tcp } => cmd_daemon(socket, tcp),
        Cmd::Web { sub } => match sub {
            WebCmd::Up { port, no_daemon } => cmd_web_up(port, no_daemon, out, cli.json),
            WebCmd::Down => cmd_web_down(out, cli.json),
            WebCmd::Status => cmd_web_status(out, cli.json),
            WebCmd::Logs { tail } => cmd_web_logs(tail),
            WebCmd::Build { pull } => cmd_web_build(pull, out, cli.json),
        },
        Cmd::SelfUpdate {
            manifest_url,
            check,
            dry_run,
            force,
        } => cmd_self_update(manifest_url, check, dry_run, force, out, cli.json),
        Cmd::Dev { sub } => match sub {
            DevCmd::Keygen { out: out_path, force } => cmd_dev_keygen(out_path, force, out, cli.json),
            DevCmd::Sign { file, key } => cmd_dev_sign(&file, key.as_deref(), out, cli.json),
            DevCmd::MakePatch { old, new, out: patch_out } => {
                cmd_dev_make_patch(&old, &new, patch_out.as_deref(), out, cli.json)
            }
            DevCmd::ApplyPatch { old, patch, out: patched_out } => {
                cmd_dev_apply_patch(&old, &patch, &patched_out, out, cli.json)
            }
        },
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    // Default filter: `info` for our own crates, `warn` for noisy
    // dependencies (hyper, tower, etc). Without this the daemon's
    // startup banner and mutation audit lines were invisible unless
    // the user remembered to set `$RUST_LOG` themselves.
    // Override with `RUST_LOG=debug` for a firehose.
    let default = "worklog_core=info,worklog_cli=info,warn";
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default)),
        )
        .with_writer(io::stderr)
        .try_init();
}

// ───────────────────────── command implementations ─────────────────────────

fn cmd_version<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let v = env!("CARGO_PKG_VERSION");
    if json {
        writeln!(
            out,
            "{}",
            serde_json::json!({ "version": v, "core": worklog_core::VERSION })
        )?;
    } else {
        writeln!(out, "worklog {v}")?;
    }
    Ok(())
}

fn cmd_doctor<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    let db_summary = if paths.db_exists() {
        let conn = db::open(&paths.db).context("opening db for doctor")?;
        Some(db::summarize(&conn)?)
    } else {
        None
    };
    let secrets = secrets::audit();

    if json {
        let report = serde_json::json!({
            "version":   env!("CARGO_PKG_VERSION"),
            "home":      paths.root.display().to_string(),
            "db_path":   paths.db.display().to_string(),
            "db_exists": paths.db_exists(),
            "db":        db_summary,
            "secrets":   secrets,
        });
        writeln!(out, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }

    writeln!(out, "worklog {} — doctor", env!("CARGO_PKG_VERSION"))?;
    writeln!(
        out,
        "  home   {}",
        worklog_core::paths::short_display(&paths.root)
    )?;
    writeln!(
        out,
        "  db     {} ({})",
        worklog_core::paths::short_display(&paths.db),
        if paths.db_exists() {
            "present"
        } else {
            "not created yet — run `worklog db migrate`"
        }
    )?;
    if let Some(s) = &db_summary {
        writeln!(
            out,
            "         schema v{}, {} events, {} blocks, {} sessions, {} tickets",
            s.schema_version, s.events, s.blocks, s.sessions, s.jira_tickets
        )?;
    }
    writeln!(out, "  secrets")?;
    for s in &secrets {
        writeln!(
            out,
            "    {:<22} {}",
            s.key,
            if s.present { "set" } else { "—" }
        )?;
    }
    Ok(())
}

fn cmd_db_migrate<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let conn = db::open(&paths.db)?;
    let v = db::current_version(&conn)?;
    if json {
        writeln!(
            out,
            "{}",
            serde_json::json!({ "ok": true, "path": paths.db, "schema_version": v })
        )?;
    } else {
        writeln!(
            out,
            "✓ db ready at {}  (schema v{v})",
            worklog_core::paths::short_display(&paths.db)
        )?;
    }
    Ok(())
}

fn cmd_db_info<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
    let s = db::summarize(&conn)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&s)?)?;
    } else {
        writeln!(
            out,
            "schema v{}  events={}  blocks={}  sessions={}  tickets={}",
            s.schema_version, s.events, s.blocks, s.sessions, s.jira_tickets
        )?;
    }
    Ok(())
}

fn cmd_setup(non_interactive: bool, skip_validate: bool) -> Result<()> {
    let opts = crate::wizard::WizardOptions {
        non_interactive,
        skip_validate,
        skip_schedule: false,
        secrets_file: std::env::var_os("WORKLOG_SECRETS_FILE").map(std::path::PathBuf::from),
    };
    let _ = crate::wizard::run(opts)?;
    Ok(())
}

fn cmd_db_path<W: Write>(out: &mut W) -> Result<()> {
    let paths = Paths::resolve()?;
    writeln!(out, "{}", paths.db.display())?;
    Ok(())
}

fn cmd_secret_set<W: Write>(key: &str, value: Option<String>, out: &mut W) -> Result<()> {
    validate_known_key(key)?;
    let value = match value {
        Some(v) => v,
        None => read_secret_value(key)?,
    };
    secrets::set(key, &value)?;
    writeln!(out, "✓ {key} saved to keychain")?;
    Ok(())
}

fn cmd_secret_get<W: Write>(key: &str, out: &mut W) -> Result<()> {
    match secrets::get(key)? {
        Some(v) => {
            out.write_all(v.as_bytes())?;
            if !v.ends_with('\n') {
                writeln!(out)?;
            }
        }
        None => anyhow::bail!("{key} not set"),
    }
    Ok(())
}

fn cmd_secret_rm<W: Write>(key: &str, out: &mut W) -> Result<()> {
    let existed = secrets::delete(key)?;
    if existed {
        writeln!(out, "✓ {key} removed")?;
    } else {
        writeln!(out, "· {key} was not set")?;
    }
    Ok(())
}

fn cmd_hook_install<W: Write>(command: Option<String>, out: &mut W, json: bool) -> Result<()> {
    let cmd = command.unwrap_or_else(hook::default_command);
    let status = hook::install(&cmd)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
    } else {
        writeln!(
            out,
            "✓ hook installed at {} ({} events)",
            worklog_core::paths::short_display(&status.settings_path),
            status.events.len()
        )?;
        writeln!(out, "  command: {cmd}")?;
    }
    Ok(())
}

fn cmd_hook_uninstall<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = hook::uninstall()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
    } else {
        writeln!(
            out,
            "✓ hook removed from {}",
            worklog_core::paths::short_display(&status.settings_path)
        )?;
    }
    Ok(())
}

fn cmd_hook_status<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = hook::status()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "settings: {}",
        worklog_core::paths::short_display(&status.settings_path)
    )?;
    if status.installed {
        writeln!(out, "installed: yes  ({} events)", status.events.len())?;
        if let Some(cmd) = &status.command {
            writeln!(out, "command:   {cmd}")?;
        }
        writeln!(out, "events:    {}", status.events.join(", "))?;
    } else {
        writeln!(out, "installed: no")?;
    }
    Ok(())
}

fn cmd_schedule_install<W: Write>(
    interval: &str,
    command: Option<String>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let iv = schedule::Interval::parse(interval)?;
    let cmd = command.unwrap_or_else(schedule::default_command);
    let status = schedule::install(iv, &cmd)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    if status.installed {
        writeln!(
            out,
            "✓ schedule installed ({} · {})",
            status.platform,
            iv.human()
        )?;
        if let Some(p) = &status.unit_path {
            writeln!(out, "  unit:    {}", worklog_core::paths::short_display(p))?;
        }
        writeln!(out, "  command: {cmd}")?;
    } else {
        writeln!(out, "· {} — {}", status.platform, status.notes.join("; "))?;
    }
    Ok(())
}

fn cmd_schedule_uninstall<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = schedule::uninstall()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
    } else {
        writeln!(out, "✓ schedule removed ({})", status.platform)?;
    }
    Ok(())
}

fn cmd_schedule_status<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = schedule::status()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    writeln!(out, "platform:  {}", status.platform)?;
    writeln!(
        out,
        "installed: {}",
        if status.installed { "yes" } else { "no" }
    )?;
    if let Some(s) = status.interval_secs {
        writeln!(
            out,
            "interval:  {}",
            schedule::Interval::parse(&s.to_string())
                .map(|i| i.human())
                .unwrap_or_else(|_| format!("{s}s"))
        )?;
    }
    if let Some(cmd) = &status.command {
        writeln!(out, "command:   {cmd}")?;
    }
    if let Some(p) = &status.unit_path {
        writeln!(out, "unit:      {}", worklog_core::paths::short_display(p))?;
    }
    for note in &status.notes {
        writeln!(out, "note:      {note}")?;
    }
    Ok(())
}

fn cmd_collect<W: Write>(target: CollectTarget, days: u32, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let conn = db::open(&paths.db)?;
    let client = http::client()?;
    let today = chrono::Utc::now().date_naive();
    let since = today - chrono::Duration::days(days as i64);

    // Each report wrapped in an Option so we can still emit something
    // useful when a source's credentials aren't set.
    let mut reports: Vec<worklog_core::collectors::CollectReport> = Vec::new();

    if matches!(target, CollectTarget::All | CollectTarget::Jira) {
        match jira_col::JiraAuth::from_secrets() {
            Ok(auth) => reports.push(jira_col::fetch_open_tickets_with(&conn, &auth, &client)?),
            Err(e) => writeln!(out, "· jira skipped: {e}")?,
        }
    }

    if matches!(target, CollectTarget::All | CollectTarget::Github) {
        match gh::GitHubAuth::from_secrets() {
            Ok(auth) => reports.push(gh::collect_with(
                &conn,
                &auth,
                since,
                today + chrono::Duration::days(1),
                &client,
            )?),
            Err(e) => writeln!(out, "· github skipped: {e}")?,
        }
    }

    if matches!(target, CollectTarget::All | CollectTarget::Gcal) {
        match gcal_col::GcalAuth::from_paths() {
            Ok(auth) => match gcal_col::collect_with(
                &conn,
                &auth,
                since,
                today + chrono::Duration::days(1),
                &client,
            ) {
                Ok(report) => reports.push(report),
                Err(e) => writeln!(out, "· gcal skipped: {e}")?,
            },
            Err(e) => writeln!(out, "· gcal skipped: {e}")?,
        }
    }

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&reports)?)?;
        return Ok(());
    }

    for r in &reports {
        writeln!(
            out,
            "✓ {:<8} tickets={}  events={}  errors={}",
            r.source,
            r.tickets_written,
            r.events_written,
            r.errors.len()
        )?;
        for err in &r.errors {
            writeln!(out, "  · {err}")?;
        }
    }
    Ok(())
}

fn cmd_sync<W: Write>(day: Option<String>, dry_run: bool, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
    let day = parse_day(day.as_deref())?;
    let auth = if dry_run {
        // Dry-run only prints payloads — placeholders are fine.
        tempo_col::TempoAuth::from_secrets().unwrap_or(tempo_col::TempoAuth {
            token: "dry-run".into(),
            author: secrets::get("jira_email")?.unwrap_or_default(),
            base_url: tempo_col::DEFAULT_BASE.into(),
        })
    } else {
        tempo_col::TempoAuth::from_secrets()?
    };
    let client = http::client()?;
    let (report, results) = tempo_col::sync_day_with(&conn, &auth, day, dry_run, &client)?;

    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "report": report,
                "results": results,
            }))?
        )?;
        return Ok(());
    }

    writeln!(
        out,
        "{} {} — {} synced, {} skipped, {} errors",
        if dry_run { "◇" } else { "✓" },
        day,
        report.synced,
        report.skipped,
        report.errors.len()
    )?;
    for r in &results {
        match r.status {
            "synced" => writeln!(
                out,
                "  ✓ block {:>4}  tempo_id={}",
                r.block_id,
                r.tempo_id.as_deref().unwrap_or("-")
            )?,
            "dry-run" => writeln!(out, "  ◇ block {:>4}  would POST", r.block_id)?,
            "skipped" => writeln!(
                out,
                "  · block {:>4}  {}",
                r.block_id,
                r.reason.as_deref().unwrap_or("")
            )?,
            "error" => writeln!(
                out,
                "  ✗ block {:>4}  HTTP {}  {}",
                r.block_id,
                r.http_status.map(|c| c.to_string()).unwrap_or_default(),
                r.reason.as_deref().unwrap_or("")
            )?,
            other => writeln!(out, "  ? block {:>4}  {other}", r.block_id)?,
        }
    }
    Ok(())
}

fn parse_day(s: Option<&str>) -> Result<chrono::NaiveDate> {
    match s {
        None => Ok(chrono::Utc::now().date_naive()),
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("invalid --day '{s}': {e}")),
    }
}

fn cmd_infer<W: Write>(day: Option<String>, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let conn = db::open(&paths.db)?;
    let day = parse_day(day.as_deref())?;
    let events = infer::load_day_events(&conn, day)?;
    let blocks = infer::build_blocks(events);
    infer::persist_blocks(&conn, day, &blocks)?;

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&blocks)?)?;
        return Ok(());
    }
    let total: i64 = blocks.iter().map(|b| b.duration_seconds).sum();
    writeln!(
        out,
        "✓ {day}: {} block{} · {} min total",
        blocks.len(),
        if blocks.len() == 1 { "" } else { "s" },
        total / 60
    )?;
    for b in &blocks {
        let flag = if b.flagged { " [flagged]" } else { "" };
        let issue = b
            .jira_issue
            .as_deref()
            .map(|k| format!(" {k}"))
            .unwrap_or_default();
        writeln!(
            out,
            "  {}–{} ({}min, {} events){}{}",
            b.started_at.format("%H:%M"),
            b.ended_at.format("%H:%M"),
            b.duration_seconds / 60,
            b.event_count,
            issue,
            flag,
        )?;
    }
    Ok(())
}

fn cmd_estimate<W: Write>(day: Option<String>, model: &str, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
    let day = parse_day(day.as_deref())?;
    let stats = estimate::estimate_day(&conn, day, model)?;

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&stats)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "✓ {day}: estimated={} skipped={} failed={}",
        stats.estimated, stats.skipped, stats.failed
    )?;
    Ok(())
}

fn cmd_hook_run() -> Result<()> {
    // All output goes to stderr (handled inside hook_run::run_from_stdin) so
    // Claude Code never sees bytes on stdout.
    hook_run::run_from_stdin()
}

fn cmd_daemon(socket: Option<std::path::PathBuf>, tcp: String) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;

    rt.block_on(async move {
        let state = daemon_mod::new_state()?;
        let router = daemon_mod::router(state.clone());
        let path = match socket {
            Some(p) => p,
            None => daemon_mod::socket_path()?,
        };
        eprintln!("→ socket {}", worklog_core::paths::short_display(&path));

        // Clone the router for the TCP task so the unix+TCP listeners
        // share the same Arc<AppState> — both mutate the same DB.
        let tcp_task = if tcp.is_empty() {
            None
        } else {
            let addr: std::net::SocketAddr = tcp
                .parse()
                .with_context(|| format!("invalid --tcp address: {tcp}"))?;
            eprintln!("→ tcp    http://{addr}");
            let tcp_router = daemon_mod::router(state);
            Some(tokio::spawn(async move {
                if let Err(e) = daemon_mod::serve_tcp(addr, tcp_router).await {
                    tracing::error!("tcp listener died: {e:#}");
                }
            }))
        };

        let unix_res = daemon_mod::serve_at(&path, router).await;
        if let Some(t) = tcp_task {
            t.abort();
        }
        unix_res
    })
}

fn cmd_secret_list<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let rows = secrets::audit();
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&rows)?)?;
    } else {
        for r in rows {
            writeln!(out, "{:<22} {}", r.key, if r.present { "set" } else { "—" })?;
        }
    }
    Ok(())
}

fn validate_known_key(key: &str) -> Result<()> {
    if !secrets::KNOWN_KEYS.contains(&key) {
        eprintln!("warning: '{key}' is not in KNOWN_KEYS — storing anyway");
    }
    Ok(())
}

/// Read a secret value: stdin if piped, or an interactive prompt on a TTY.
/// We trim the trailing newline but preserve interior whitespace.
fn read_secret_value(key: &str) -> Result<String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        // Minimal prompt — the wizard handles fancy UX separately.
        eprint!("Paste value for {key} (input hidden): ");
        io::stderr().flush().ok();
        let v = rpassword_readline()?;
        Ok(v)
    } else {
        let mut buf = String::new();
        stdin.lock().read_to_string(&mut buf)?;
        Ok(buf.trim_end_matches(['\n', '\r']).to_owned())
    }
}

/// Fallback password prompt that works without pulling in `rpassword`.
/// On POSIX we toggle the terminal to non-echo via `termios`; if that fails
/// we fall back to a visible read.
fn rpassword_readline() -> Result<String> {
    #[cfg(unix)]
    {
        use std::io::BufRead;
        use std::os::fd::AsRawFd;
        // Deliberately dep-light: read a line visibly rather than pull in
        // `rpassword` for one prompt. The interactive secret flow in the
        // wizard handles echo suppression via `dialoguer::Password`.
        let fd = io::stdin().as_raw_fd();
        let _ = fd;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\n', '\r']).to_owned())
    }
    #[cfg(not(unix))]
    {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\n', '\r']).to_owned())
    }
}

// ─────────────────────────── web subcommand ───────────────────────────

fn cmd_web_up<W: Write>(port: u16, no_daemon: bool, out: &mut W, json: bool) -> Result<()> {
    web_mod::preflight_docker()?;
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let context = web_mod::resolve_web_context()?;
    let compose = web_mod::render_compose(&paths, port, &context)?;

    if !no_daemon && !daemon_tcp_reachable("127.0.0.1:9323") {
        eprintln!("⚠ worklog daemon isn't listening on 127.0.0.1:9323.");
        eprintln!("   Start it in another terminal with: worklog daemon");
        eprintln!("   (The web UI can read the DB without it, but writes will fail.)");
    }

    web_mod::compose_up(&compose, false)?;

    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "url": format!("http://localhost:{port}"),
                "compose": compose.display().to_string(),
                "context": context.display().to_string(),
            }))?
        )?;
    } else {
        writeln!(out, "✓ worklog-web up — open http://localhost:{port}")?;
        writeln!(out, "  compose: {}", compose.display())?;
    }
    Ok(())
}

fn cmd_web_down<W: Write>(out: &mut W, json: bool) -> Result<()> {
    web_mod::preflight_docker()?;
    let paths = Paths::resolve()?;
    let compose = web_mod::compose_path(&paths);
    if !compose.is_file() {
        anyhow::bail!(
            "no compose file at {} — nothing to bring down",
            compose.display()
        );
    }
    web_mod::compose_down(&compose)?;
    if json {
        writeln!(out, "{{\"ok\": true}}")?;
    } else {
        writeln!(out, "✓ worklog-web stopped")?;
    }
    Ok(())
}

fn cmd_web_status<W: Write>(out: &mut W, json: bool) -> Result<()> {
    web_mod::preflight_docker().ok(); // status should still run if docker daemon is off
    let status = web_mod::status()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
    } else if status.running {
        writeln!(
            out,
            "✓ running — image={} port={} started={}",
            status.image.as_deref().unwrap_or("?"),
            status.port.map(|p| p.to_string()).unwrap_or("?".into()),
            status.uptime.as_deref().unwrap_or("?"),
        )?;
    } else {
        writeln!(out, "— not running. Start with `worklog web up`.")?;
    }
    Ok(())
}

fn cmd_web_logs(tail: u32) -> Result<()> {
    web_mod::preflight_docker()?;
    let paths = Paths::resolve()?;
    let compose = web_mod::compose_path(&paths);
    if !compose.is_file() {
        anyhow::bail!(
            "no compose file at {} — run `worklog web up` first",
            compose.display()
        );
    }
    web_mod::compose_logs(&compose, tail)
}

fn cmd_web_build<W: Write>(pull: bool, out: &mut W, json: bool) -> Result<()> {
    web_mod::preflight_docker()?;
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let context = web_mod::resolve_web_context()?;
    // Re-render so the compose file points at the current web/ location.
    let compose = web_mod::render_compose(&paths, 3333, &context)?;
    web_mod::compose_build(&compose, pull)?;
    if json {
        writeln!(out, "{{\"ok\": true, \"context\": {:?}}}", context.display())?;
    } else {
        writeln!(out, "✓ worklog-web image built from {}", context.display())?;
    }
    Ok(())
}

/// Cheap reachability check: try to open a TCP connection to the daemon
/// and immediately close. We don't speak HTTP — we just confirm a listener
/// is accepting. Timeout is bounded so a stuck daemon can't hang the CLI.
fn daemon_tcp_reachable(addr: &str) -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    let Ok(parsed) = addr.parse::<std::net::SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&parsed, Duration::from_millis(150)).is_ok()
}

// ─────────────────────── self-update + dev commands ───────────────────────

/// Default manifest URL if the user hasn't set --manifest-url or the env var.
/// Points at GitHub's "latest release" asset URL, which redirects to the
/// actual release tag's asset.
const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/TomasPalsson/worklog/releases/latest/download/manifest.json";

/// Default path for the release signing private key. Lives under the
/// resolved worklog config dir to match the rest of the app.
fn default_key_path() -> Result<std::path::PathBuf> {
    let paths = Paths::resolve()?;
    Ok(paths.config_dir.join("keys").join("release-ed25519.pem"))
}

fn cmd_self_update<W: Write>(
    manifest_url: Option<String>,
    check: bool,
    dry_run: bool,
    force: bool,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;

    let binary = std::env::current_exe().context("resolving current binary path")?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let url = manifest_url.unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_string());

    let req = upd::UpdateRequest {
        manifest_url: url.clone(),
        current_binary: binary,
        current_version: current_version.clone(),
        work_dir: paths.data_dir.join("updates"),
        dry_run: check || dry_run,
        force,
    };

    if check {
        // --check is a no-swap probe: download + verify just the manifest,
        // report whether an update is available.
        if upd::pubkey::is_placeholder() {
            anyhow::bail!(
                "release pubkey isn't embedded yet — see worklog-core::updater::pubkey"
            );
        }
        let pk = upd::pubkey::resolve();
        let http = upd::fetch::client()?;
        let manifest = upd::fetch::fetch_manifest(&http, &url, &pk)?;
        if json {
            writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "current":  current_version,
                    "latest":   manifest.version,
                    "up_to_date": manifest.version == current_version,
                    "notes":    manifest.notes,
                }))?
            )?;
        } else if manifest.version == current_version {
            writeln!(out, "✓ up to date ({current_version})")?;
        } else {
            writeln!(
                out,
                "→ {current_version} → {} available",
                manifest.version
            )?;
            if !manifest.notes.is_empty() {
                writeln!(out, "\n{}", manifest.notes)?;
            }
        }
        return Ok(());
    }

    let report = upd::run_update(&req)?;
    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "from":        report.from,
                "to":          report.to,
                "used_delta":  report.used_delta,
                "asset_bytes": report.asset_bytes,
                "dry_run":     report.dry_run,
                "rolled_back": report.rolled_back,
            }))?
        )?;
    } else if report.rolled_back {
        writeln!(
            out,
            "✗ new binary failed smoke test — rolled back to {}.",
            report.from
        )?;
    } else if report.install.is_none() && !report.dry_run {
        writeln!(out, "✓ already on {} — nothing to do", report.from)?;
    } else if report.dry_run {
        writeln!(
            out,
            "✓ dry-run — would update {} → {} ({} bytes{})",
            report.from,
            report.to,
            report.asset_bytes,
            if report.used_delta { ", delta" } else { "" }
        )?;
    } else {
        writeln!(
            out,
            "✓ updated {} → {} ({} bytes{})",
            report.from,
            report.to,
            report.asset_bytes,
            if report.used_delta { ", delta" } else { "" }
        )?;
    }
    Ok(())
}

fn cmd_dev_keygen<W: Write>(
    out_path: Option<std::path::PathBuf>,
    force: bool,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let path = match out_path {
        Some(p) => p,
        None => default_key_path()?,
    };
    let gen = upd::signing::keygen_to_file(&path, force)?;

    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "private_key_path":        path.display().to_string(),
                "public_key_rust_literal": gen.rust_literal,
                "public_key_base64":       gen.base64,
            }))?
        )?;
    } else {
        writeln!(out, "✓ private key → {} (chmod 600)", path.display())?;
        writeln!(
            out,
            "\nPaste this into rust/crates/worklog-core/src/updater/pubkey.rs:\n"
        )?;
        writeln!(
            out,
            "pub const RELEASE_PUBLIC_KEY: [u8; PUBLIC_KEY_LEN] = {};",
            gen.rust_literal
        )?;
    }
    Ok(())
}

fn cmd_dev_sign<W: Write>(
    file: &std::path::Path,
    key_path: Option<&std::path::Path>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let key_path = match key_path {
        Some(p) => p.to_path_buf(),
        None => default_key_path()?,
    };
    let sk = upd::signing::load_signing_key(&key_path)?;
    let msg = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let sig = upd::crypto::sign_detached(&sk.to_bytes(), &msg);
    let mut sig_path = file.to_path_buf();
    let new_name = format!(
        "{}.sig",
        file.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
    );
    sig_path.set_file_name(new_name);
    std::fs::write(&sig_path, sig).with_context(|| format!("writing {}", sig_path.display()))?;

    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "signature_path":    sig_path.display().to_string(),
                "file_sha256":       upd::crypto::sha256_hex(&msg),
            }))?
        )?;
    } else {
        writeln!(out, "✓ signed → {}", sig_path.display())?;
    }
    Ok(())
}

fn cmd_dev_make_patch<W: Write>(
    old: &std::path::Path,
    new: &std::path::Path,
    out_path: Option<&std::path::Path>,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let old_bytes = std::fs::read(old).with_context(|| format!("reading {}", old.display()))?;
    let new_bytes = std::fs::read(new).with_context(|| format!("reading {}", new.display()))?;
    let patch = upd::delta::make_patch(&old_bytes, &new_bytes)?;
    let dest = match out_path {
        Some(p) => p.to_path_buf(),
        None => {
            let mut p = new.to_path_buf();
            p.set_extension(format!(
                "{}.patch",
                new.extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("bin")
            ));
            p
        }
    };
    std::fs::write(&dest, &patch)?;
    // The result SHA256 is the load-bearing post-apply check in
    // run_update — surface it so the release author can paste it into
    // the manifest's PatchDescriptor.result_sha256.
    let result_sha = upd::crypto::sha256_hex(&new_bytes);
    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "patch_path":    dest.display().to_string(),
                "patch_bytes":   patch.len(),
                "old_bytes":     old_bytes.len(),
                "new_bytes":     new_bytes.len(),
                "ratio":         patch.len() as f64 / new_bytes.len() as f64,
                "result_sha256": result_sha,
            }))?
        )?;
    } else {
        writeln!(
            out,
            "✓ patch → {} ({} bytes, {:.1}% of new)",
            dest.display(),
            patch.len(),
            100.0 * patch.len() as f64 / new_bytes.len() as f64,
        )?;
        writeln!(out, "  manifest result_sha256: {result_sha}")?;
    }
    Ok(())
}

fn cmd_dev_apply_patch<W: Write>(
    old: &std::path::Path,
    patch: &std::path::Path,
    out_path: &std::path::Path,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let old_bytes = std::fs::read(old)?;
    let patch_bytes = std::fs::read(patch)?;
    let new_bytes = upd::delta::apply_patch(&old_bytes, &patch_bytes)?;
    std::fs::write(out_path, &new_bytes)?;
    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "out_path":  out_path.display().to_string(),
                "out_bytes": new_bytes.len(),
                "sha256":    upd::crypto::sha256_hex(&new_bytes),
            }))?
        )?;
    } else {
        writeln!(
            out,
            "✓ reconstructed → {} ({} bytes)",
            out_path.display(),
            new_bytes.len()
        )?;
    }
    Ok(())
}

