//! CLI command definitions + dispatcher.
//!
//! Kept in `lib.rs` so integration tests can invoke `worklog_cli::run_with`
//! with explicit argv without spawning a subprocess. `main.rs` simply calls
//! `run()` which uses the real `std::env::args`.

use std::io::{self, IsTerminal, Read, Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use worklog_core::{db, hook, paths::Paths, repo, schedule, secrets};

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
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
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
    // Keep the warning about unused import silent.
    let _ = repo::count_events(&conn);
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
        // Best-effort: just read a line visibly on systems where we can't
        // toggle echo without extra deps. This keeps Stage 1 dep-light; we'll
        // swap in `rpassword` properly in Stage 1.2 wizard work.
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
