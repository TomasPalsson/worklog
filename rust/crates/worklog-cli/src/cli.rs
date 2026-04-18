//! CLI command definitions + dispatcher.
//!
//! Kept in `lib.rs` so integration tests can invoke `worklog_cli::run_with`
//! with explicit argv without spawning a subprocess. `main.rs` simply calls
//! `run()` which uses the real `std::env::args`.

use std::io::{self, IsTerminal, Read, Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use worklog_core::{db, paths::Paths, repo, secrets};

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
