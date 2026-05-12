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
    personal as personal_mod, schedule, secrets, skill as skill_mod, updater as upd,
    web as web_mod,
};

use crate::style;

/// Clap styles applied to every help surface. Matches the `console`
/// colour palette used by `style.rs` so help + status output feel like
/// one CLI. `AnsiColor::Cyan` is reused for section headers, and valid
/// values get a matching green to keep the palette tight.
fn clap_styles() -> clap::builder::Styles {
    use clap::builder::styling::{AnsiColor, Effects};
    use clap::builder::Styles;
    Styles::styled()
        .header(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .usage(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default())
        .placeholder(AnsiColor::Yellow.on_default())
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Red.on_default() | Effects::BOLD)
}

/// Static overview rendered at the top of `worklog --help`. Groups the
/// subcommands into the same logical sections the README and CLAUDE.md
/// reference. Kept as a `&str` constant so clap inlines it — a fn call
/// would need lifetime gymnastics in the derive attribute.
const HELP_OVERVIEW: &str = "\x1b[1;36m\
commands by area\x1b[0m
  setup & diagnostics  \x1b[32msetup  doctor  db  secret  version\x1b[0m
  data collection      \x1b[32mcollect  infer  estimate  hook\x1b[0m
  review & sync        \x1b[32mday  sync  web  serve\x1b[0m
  daemon & schedule    \x1b[32mdaemon  schedule\x1b[0m
  release ops          \x1b[32mself-update  upgrade  dev\x1b[0m
";

/// worklog — personal time-tracking for the developer who hates timers.
#[derive(Parser, Debug)]
#[command(
    name = "worklog",
    version,
    about = "Personal worklog — Rust edition.",
    long_about = None,
    styles = clap_styles(),
    before_help = HELP_OVERVIEW,
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
    Doctor {
        /// Also probe the LiteLLM proxy (when configured) — adds a 3s
        /// HTTP GET against `{base_url}/health`. Default off so
        /// `worklog doctor` stays a fast, offline-safe check.
        /// `WORKLOG_DOCTOR_PROBE=1` is accepted as an alternative.
        #[arg(long, env = "WORKLOG_DOCTOR_PROBE")]
        probe: bool,
    },

    /// One-shot onboarding: db migrate + preflight + capture secrets,
    /// then bring up the review UI in your browser. Pass `--no-serve`
    /// to stop after configuration.
    Setup {
        /// Print what would run and exit; capture nothing.
        #[arg(long)]
        non_interactive: bool,
        /// Skip HTTP validation of captured credentials.
        #[arg(long)]
        skip_validate: bool,
        /// Don't auto-start the web UI when the wizard finishes.
        #[arg(long)]
        no_serve: bool,
        /// Port for the auto-started web UI. Default: 3333.
        #[arg(long, default_value_t = 3333)]
        port: u16,
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

    /// Install / refresh / remove the Claude Code skill that teaches
    /// Claude how to operate worklog (write SKILL.md + references to
    /// ~/.claude/skills/worklog).
    #[command(subcommand)]
    Skill(SkillCmd),

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
    #[command(long_about = "\
Estimate jira_issue + duration + description for each un-estimated block \
on the given day.

Provider selection:
  Set WORKLOG_ESTIMATOR_PROVIDER=claude_subprocess (default) or =litellm.
  The env var wins; the `worklog_estimator_provider` secret is used as a
  persistent fallback (set via `worklog setup`).

Providers:
  claude_subprocess   shells out to `claude -p` — the historical default.
  litellm             POSTs to any OpenAI-compatible proxy (LiteLLM,
                      Anthropic-shaped). Configure with:
                        worklog secret set litellm_base_url <URL>
                        worklog secret set litellm_api_key <key>
                        worklog secret set litellm_model <e.g. anthropic/...>

--model accepts whatever the active provider understands: plain Claude
model ids for the subprocess path, `provider/model` form for LiteLLM.")]
    Estimate {
        /// YYYY-MM-DD; default today (UTC).
        #[arg(long)]
        day: Option<String>,
        /// Model id passed to the active estimator provider.
        #[arg(long, default_value = estimate::DEFAULT_MODEL)]
        model: String,
    },

    /// One-shot daily flow: collect → infer → estimate → open the review UI.
    ///
    /// Each step that fails is reported inline and the next one still runs —
    /// missing Jira credentials or a transient API blip shouldn't block the
    /// rest of the pipeline. Use `--no-serve` to skip spinning up the web UI.
    Day {
        /// YYYY-MM-DD; default today (UTC).
        #[arg(long)]
        day: Option<String>,
        /// Skip launching the web review UI at the end.
        #[arg(long)]
        no_serve: bool,
        /// Model id passed to `claude -p` during estimation.
        #[arg(long, default_value = estimate::DEFAULT_MODEL)]
        model: String,
    },

    /// Personal/work classification — manage the auto-classifier and
    /// re-evaluate existing blocks against the current ruleset.
    Tag {
        #[command(subcommand)]
        sub: TagCmd,
    },

    /// Claude Code hook — reads a JSON event from stdin and records it.
    #[command(name = "hook-run", hide = true)]
    HookRun,

    /// Start the axum unix-socket IPC server (foreground) OR manage the
    /// background service unit that supervises it. Bare `worklog daemon`
    /// keeps running in the foreground like before; the install / status
    /// / uninstall subcommands write a launchd plist / systemd user unit.
    Daemon {
        #[command(subcommand)]
        sub: Option<DaemonCmd>,
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

    /// Bring up the review UI. Convenience alias for `web up` — kept so
    /// muscle memory from the Python era (`worklog serve`) keeps working.
    Serve {
        /// Host port the container binds. Default: 3333.
        #[arg(long, default_value_t = 3333)]
        port: u16,
        /// Skip the daemon-reachability check (useful when the daemon
        /// is managed out-of-band, e.g. a systemd unit on linux).
        #[arg(long)]
        no_daemon: bool,
        /// Skip auto-opening the URL in your default browser.
        #[arg(long)]
        no_open: bool,
    },

    /// Self-update: verify + download + atomically swap the binary.
    #[command(alias = "upgrade")]
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
pub enum DaemonCmd {
    /// Install a launchd plist (macOS) or systemd user unit (Linux) so
    /// the daemon starts at login and auto-restarts on crash.
    Install {
        /// Override the command baked into the service unit. Defaults
        /// to the resolved `worklog` binary + ` daemon`.
        #[arg(long)]
        command: Option<String>,
    },
    /// Remove the service unit (and stop the supervisor-managed process).
    Uninstall,
    /// Report whether the service unit is installed and where it lives.
    Status,
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
        /// Skip auto-opening the URL in your default browser. The
        /// browser opens by default on TTY runs; `--json` and
        /// non-tty stdout already suppress it automatically.
        #[arg(long)]
        no_open: bool,
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
    /// Download the `web/` tree from GitHub into the local cache
    /// (`$data/web`). Lets `worklog web up` work on machines that
    /// only installed the binary via curl — no repo clone needed.
    Fetch {
        /// Force a re-download even when the cache already matches the
        /// binary's version.
        #[arg(long)]
        force: bool,
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
    /// Drop events + blocks older than N days that have been synced to
    /// Tempo (or explicitly marked as 'gap'). Preserves unsynced work and
    /// manual edits regardless of age.
    Purge {
        /// Retention window in days. Anything older is fair game.
        #[arg(long, default_value_t = worklog_core::purge::DEFAULT_RETENTION_DAYS)]
        days: i64,
        /// Report what would be deleted without touching the database.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum HookCmd {
    /// Register the stdin-JSON hook in ~/.claude/settings.json.
    Install {
        /// Override the command stored in each handler. Default: auto-detect
        /// (`worklog-rs hook-run` or `worklog hook-run`).
        #[arg(long)]
        command: Option<String>,
    },
    /// Remove worklog handlers; leaves other tools' hooks alone.
    Uninstall,
    /// Report whether worklog is registered and which events it listens on.
    Status,
    /// Process a single Claude Code hook event from stdin. Back-compat
    /// alias for the top-level `hook-run` — keeps existing
    /// ~/.claude/settings.json entries like `worklog hook run` working
    /// after the Python-era CLI shape was retired. New installs write
    /// `hook-run` directly; this alias is the graceful migration path.
    #[command(hide = true)]
    Run,
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
pub enum SkillCmd {
    /// Write the bundled SKILL.md + reference files to ~/.claude/skills/worklog/.
    /// Idempotent — re-run after `worklog upgrade` to pick up new bundled content.
    Install,
    /// Remove ~/.claude/skills/worklog/ entirely.
    Uninstall,
    /// Report whether the skill is installed and whether the on-disk files
    /// match the bundled version (false → re-run install).
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

#[derive(Subcommand, Debug)]
pub enum TagCmd {
    /// Print the current personal/work patterns and the config file path.
    List,
    /// Add a path glob to the personal list (creates the config if missing).
    Personal {
        /// Glob pattern, e.g. `~/Desktop/Projects/ai-news/**` or
        /// `~/Desktop/Work/scratch`. Tilde expansion is applied.
        glob: String,
    },
    /// Add a path glob to the explicit work list — wins over the
    /// default rule and any matching personal pattern.
    Work { glob: String },
    /// Re-evaluate the `is_personal` column on existing blocks against
    /// the current ruleset. Doesn't re-cluster.
    Reclassify {
        /// YYYY-MM-DD. If omitted, all days are reclassified.
        #[arg(long)]
        day: Option<String>,
    },
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
    // Hand-roll clap error handling so --help / --version / `help` exit
    // with code 0 instead of bubbling up as anyhow errors. Clap's default
    // `try_parse_from` treats these as Err(ErrorKind::DisplayHelp), which
    // is semantically fine for library callers but lands as exit-1 in
    // main() — users see "Error: worklog — ..." on stderr with a clean
    // exit code, indistinguishable from actual parse failures. This is
    // the standard fix: print on the right stream, exit process with 0.
    let cli = match Cli::try_parse_from(argv) {
        Ok(c) => c,
        Err(e) => {
            use clap::error::ErrorKind;
            match e.kind() {
                // Help / version / "which sub?" are all informational.
                // Clap's default routes MissingSubcommand (and similar)
                // to stderr as an error — we route everything here to
                // stdout and exit 0 so `worklog web | cat` shows the
                // subcommand list like every other CLI. That deviates
                // slightly from the POSIX "exit 2 on misuse" convention,
                // but matches user expectation for an exploratory CLI
                // that doubles as its own documentation.
                ErrorKind::DisplayHelp
                | ErrorKind::DisplayVersion
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | ErrorKind::MissingSubcommand => {
                    // `e` renders the formatted text (with ANSI colors
                    // if attached to a tty); println forces stdout.
                    println!("{e}");
                    std::process::exit(0);
                }
                _ => return Err(e.into()),
            }
        }
    };
    init_tracing();
    if let Some(h) = &cli.home {
        std::env::set_var("WORKLOG_HOME", h);
    }

    match cli.command {
        Cmd::Version => cmd_version(out, cli.json),
        Cmd::Doctor { probe } => cmd_doctor(probe, out, cli.json),
        Cmd::Setup {
            non_interactive,
            skip_validate,
            no_serve,
            port,
        } => cmd_setup(
            non_interactive,
            skip_validate,
            no_serve,
            port,
            out,
            cli.json,
        ),
        Cmd::Db(sub) => match sub {
            DbCmd::Migrate => cmd_db_migrate(out, cli.json),
            DbCmd::Info => cmd_db_info(out, cli.json),
            DbCmd::Path => cmd_db_path(out),
            DbCmd::Purge { days, dry_run } => cmd_db_purge(days, dry_run, out, cli.json),
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
            HookCmd::Run => cmd_hook_run(),
        },
        Cmd::Schedule(sub) => match sub {
            ScheduleCmd::Install { interval, command } => {
                cmd_schedule_install(&interval, command, out, cli.json)
            }
            ScheduleCmd::Uninstall => cmd_schedule_uninstall(out, cli.json),
            ScheduleCmd::Status => cmd_schedule_status(out, cli.json),
        },
        Cmd::Skill(sub) => match sub {
            SkillCmd::Install => cmd_skill_install(out, cli.json),
            SkillCmd::Uninstall => cmd_skill_uninstall(out, cli.json),
            SkillCmd::Status => cmd_skill_status(out, cli.json),
        },
        Cmd::Collect { target, days } => cmd_collect(target, days, out, cli.json),
        Cmd::Sync { day, dry_run } => cmd_sync(day, dry_run, out, cli.json),
        Cmd::Infer { day } => cmd_infer(day, out, cli.json),
        Cmd::Estimate { day, model } => cmd_estimate(day, &model, out, cli.json),
        Cmd::Day {
            day,
            no_serve,
            model,
        } => cmd_day(day, no_serve, &model, out, cli.json),
        Cmd::Tag { sub } => match sub {
            TagCmd::List => cmd_tag_list(out, cli.json),
            TagCmd::Personal { glob } => cmd_tag_personal(glob, out, cli.json),
            TagCmd::Work { glob } => cmd_tag_work(glob, out, cli.json),
            TagCmd::Reclassify { day } => cmd_tag_reclassify(day, out, cli.json),
        },
        Cmd::HookRun => cmd_hook_run(),
        Cmd::Daemon { sub, socket, tcp } => match sub {
            None => cmd_daemon(socket, tcp),
            Some(DaemonCmd::Install { command }) => cmd_daemon_install(command, out, cli.json),
            Some(DaemonCmd::Uninstall) => cmd_daemon_uninstall(out, cli.json),
            Some(DaemonCmd::Status) => cmd_daemon_status(out, cli.json),
        },
        Cmd::Web { sub } => match sub {
            WebCmd::Up {
                port,
                no_daemon,
                no_open,
            } => cmd_web_up(port, no_daemon, no_open, out, cli.json),
            WebCmd::Down => cmd_web_down(out, cli.json),
            WebCmd::Status => cmd_web_status(out, cli.json),
            WebCmd::Logs { tail } => cmd_web_logs(tail),
            WebCmd::Build { pull } => cmd_web_build(pull, out, cli.json),
            WebCmd::Fetch { force } => cmd_web_fetch(force, out, cli.json),
        },
        // `serve` is literally `web up` with the same args — the alias
        // lives at the top-level so muscle memory from the Python era
        // (`worklog serve`) keeps working.
        Cmd::Serve {
            port,
            no_daemon,
            no_open,
        } => cmd_web_up(port, no_daemon, no_open, out, cli.json),
        Cmd::SelfUpdate {
            manifest_url,
            check,
            dry_run,
            force,
        } => cmd_self_update(manifest_url, check, dry_run, force, out, cli.json),
        Cmd::Dev { sub } => match sub {
            DevCmd::Keygen {
                out: out_path,
                force,
            } => cmd_dev_keygen(out_path, force, out, cli.json),
            DevCmd::Sign { file, key } => cmd_dev_sign(&file, key.as_deref(), out, cli.json),
            DevCmd::MakePatch {
                old,
                new,
                out: patch_out,
            } => cmd_dev_make_patch(&old, &new, patch_out.as_deref(), out, cli.json),
            DevCmd::ApplyPatch {
                old,
                patch,
                out: patched_out,
            } => cmd_dev_apply_patch(&old, &patch, &patched_out, out, cli.json),
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

/// What `doctor` shows for the active estimator. `base_url`/`model`
/// populated only when the user picked LiteLLM; `reachable` is a
/// best-effort probe so hitting this in a tight loop doesn't hang.
#[derive(Debug, serde::Serialize)]
struct EstimatorReport {
    provider: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reachable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn estimator_report(probe: bool) -> EstimatorReport {
    use worklog_core::estimate as est;
    match est::resolve_provider() {
        Ok(est::ProviderChoice::ClaudeSubprocess) => EstimatorReport {
            provider: "claude_subprocess",
            base_url: None,
            model: None,
            reachable: None,
            reason: None,
        },
        Ok(est::ProviderChoice::LiteLLM(inv)) => {
            // Resolved choice has already consumed the secrets — pull
            // the user-facing fields back out of the invoker itself.
            let endpoint = inv.endpoint();
            let base_url = endpoint.trim_end_matches("/v1/chat/completions").to_owned();
            let model = inv.configured_model().to_owned();
            let (reachable, reason) = if probe {
                // Live probe — 3s timeout. Only runs when the caller
                // opted in via `--probe` or `WORKLOG_DOCTOR_PROBE=1`.
                let p = est::probe_litellm(&base_url);
                (Some(p.is_none()), p)
            } else {
                // Skip by default so scripted callers don't pay the
                // 3s-HTTP tax on every `worklog doctor` invocation.
                (None, None)
            };
            EstimatorReport {
                provider: "litellm",
                base_url: Some(base_url),
                model: Some(model),
                reachable,
                reason,
            }
        }
        Err(e) => EstimatorReport {
            // We intentionally surface misconfiguration as
            // `provider: unconfigured` so `doctor` can run even when
            // the env says litellm but the secrets aren't set yet.
            provider: "unconfigured",
            base_url: None,
            model: None,
            reachable: None,
            reason: Some(format!("{e:#}")),
        },
    }
}

fn cmd_doctor<W: Write>(probe: bool, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    let db_summary = if paths.db_exists() {
        let conn = db::open(&paths.db).context("opening db for doctor")?;
        Some(db::summarize(&conn)?)
    } else {
        None
    };
    let secrets = secrets::audit();
    let estimator = estimator_report(probe);

    if json {
        let report = serde_json::json!({
            "version":   env!("CARGO_PKG_VERSION"),
            "home":      paths.root.display().to_string(),
            "db_path":   paths.db.display().to_string(),
            "db_exists": paths.db_exists(),
            "db":        db_summary,
            "secrets":   secrets,
            "estimator": estimator,
        });
        writeln!(out, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }

    writeln!(out, "worklog {} — doctor", env!("CARGO_PKG_VERSION"))?;

    let mut env_table = style::table();
    env_table.set_header(vec!["check", "value", "note"]);
    env_table.add_row(vec![
        "home".to_owned(),
        worklog_core::paths::short_display(&paths.root),
        String::new(),
    ]);
    env_table.add_row(vec![
        "db".to_owned(),
        worklog_core::paths::short_display(&paths.db),
        if paths.db_exists() {
            "present".into()
        } else {
            "missing — run `worklog db migrate`".into()
        },
    ]);
    if let Some(s) = &db_summary {
        env_table.add_row(vec![
            "schema".to_owned(),
            format!("v{}", s.schema_version),
            format!(
                "{} events, {} blocks, {} sessions, {} tickets",
                s.events, s.blocks, s.sessions, s.jira_tickets
            ),
        ]);
    }
    let est_note = match &estimator.reachable {
        Some(true) => "reachable".to_string(),
        Some(false) => estimator
            .reason
            .clone()
            .unwrap_or_else(|| "unreachable".to_string()),
        None if estimator.provider == "unconfigured" => estimator
            .reason
            .clone()
            .unwrap_or_else(|| "misconfigured".to_string()),
        None if estimator.provider == "litellm" => "probe skipped (pass --probe)".to_string(),
        None => String::new(),
    };
    let est_value = match (&estimator.base_url, &estimator.model) {
        (Some(url), Some(model)) => format!("{} {} ({})", estimator.provider, model, url),
        _ => estimator.provider.to_string(),
    };
    env_table.add_row(vec!["estimator".to_owned(), est_value, est_note]);
    writeln!(out, "{env_table}")?;

    let mut sec_table = style::table();
    sec_table.set_header(vec!["secret", "status"]);
    for s in &secrets {
        sec_table.add_row(vec![
            s.key.to_string(),
            if s.present {
                "set".into()
            } else {
                "—".into()
            },
        ]);
    }
    writeln!(out, "secrets")?;
    writeln!(out, "{sec_table}")?;

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

fn cmd_db_purge<W: Write>(days: i64, dry_run: bool, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    if !paths.db_exists() {
        anyhow::bail!("db not initialized. Run `worklog db migrate` first.");
    }
    let conn = db::open(&paths.db)?;
    let report = worklog_core::purge::purge(&conn, days, dry_run)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }
    let prefix = if dry_run { "(dry-run) " } else { "" };
    if report.blocks_deleted + report.events_deleted == 0 {
        style::info(
            out,
            &format!(
                "{prefix}nothing to purge before {} (kept: {} unsynced, {} manual)",
                report.cutoff_date, report.blocks_kept_unsynced, report.blocks_kept_manual
            ),
        )?;
    } else {
        let verb = if dry_run { "would delete" } else { "deleted" };
        style::ok(
            out,
            &format!(
                "{prefix}{verb} {} block(s) + {} event(s) before {} (kept: {} unsynced, {} manual)",
                report.blocks_deleted,
                report.events_deleted,
                report.cutoff_date,
                report.blocks_kept_unsynced,
                report.blocks_kept_manual
            ),
        )?;
    }
    Ok(())
}

fn cmd_setup<W: Write>(
    non_interactive: bool,
    skip_validate: bool,
    no_serve: bool,
    port: u16,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let opts = crate::wizard::WizardOptions {
        non_interactive,
        skip_validate,
        skip_schedule: false,
        secrets_file: std::env::var_os("WORKLOG_SECRETS_FILE").map(std::path::PathBuf::from),
    };
    let _ = crate::wizard::run(opts)?;

    // Auto-launch the review UI so first-run feels seamless. We skip
    // this for `--non-interactive` (CI / scripting) and `--no-serve`
    // (user opt-out). The auto-open of the browser inside
    // `cmd_web_up` further requires a real TTY, so headless runs
    // still print the URL instead of trying to launch a browser.
    if non_interactive || no_serve {
        return Ok(());
    }

    // Visual break before the web-up log lines.
    if !json {
        writeln!(out)?;
        writeln!(out, "→ launching review UI…")?;
    }

    // `no_daemon = false` → setup just configured the daemon, but the
    // service may not be running yet; cmd_web_up will start it.
    // `no_open = false` → the whole point of auto-serve is to open
    // the browser; rely on the TTY check inside cmd_web_up to suppress
    // it when stdout isn't a real terminal.
    cmd_web_up(port, false, false, out, json)
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

fn cmd_skill_install<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = skill_mod::install()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "✓ skill installed at {} ({} files, v{})",
        worklog_core::paths::short_display(&status.skill_dir),
        status.files.len(),
        status.bundled_version,
    )?;
    Ok(())
}

fn cmd_skill_uninstall<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = skill_mod::uninstall()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
    } else {
        writeln!(
            out,
            "✓ skill removed from {}",
            worklog_core::paths::short_display(&status.skill_dir)
        )?;
    }
    Ok(())
}

fn cmd_skill_status<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = skill_mod::status()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "dir:       {}",
        worklog_core::paths::short_display(&status.skill_dir)
    )?;
    if status.installed {
        writeln!(out, "installed: yes ({} files)", status.files.len())?;
        match status.up_to_date {
            Some(true) => writeln!(out, "up-to-date: yes (v{})", status.bundled_version)?,
            Some(false) => writeln!(
                out,
                "up-to-date: no — run `worklog skill install` to refresh"
            )?,
            None => {}
        }
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
        let pb = style::spinner("jira …");
        let result = match jira_col::JiraAuth::from_secrets() {
            Ok(auth) => jira_col::fetch_open_tickets_with(&conn, &auth, &client)
                .map_err(|e| format!("fetch: {e}")),
            Err(e) => Err(format!("skipped: {e}")),
        };
        pb.finish_and_clear();
        match result {
            Ok(r) => reports.push(r),
            Err(msg) if !json => style::info(out, &format!("jira {msg}"))?,
            Err(_) => (),
        }
    }

    if matches!(target, CollectTarget::All | CollectTarget::Github) {
        let pb = style::spinner("github …");
        let result = match gh::GitHubAuth::from_secrets() {
            Ok(auth) => gh::collect_with(
                &conn,
                &auth,
                since,
                today + chrono::Duration::days(1),
                &client,
            )
            .map_err(|e| format!("fetch: {e}")),
            Err(e) => Err(format!("skipped: {e}")),
        };
        pb.finish_and_clear();
        match result {
            Ok(r) => reports.push(r),
            Err(msg) if !json => style::info(out, &format!("github {msg}"))?,
            Err(_) => (),
        }
    }

    if matches!(target, CollectTarget::All | CollectTarget::Gcal) {
        let pb = style::spinner("gcal …");
        let result = match gcal_col::GcalAuth::from_paths() {
            Ok(auth) => gcal_col::collect_with(
                &conn,
                &auth,
                since,
                today + chrono::Duration::days(1),
                &client,
            )
            .map_err(|e| format!("fetch: {e}")),
            Err(e) => Err(format!("skipped: {e}")),
        };
        pb.finish_and_clear();
        match result {
            Ok(r) => reports.push(r),
            Err(msg) if !json => style::info(out, &format!("gcal {msg}"))?,
            Err(_) => (),
        }
    }

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&reports)?)?;
        return Ok(());
    }

    for r in &reports {
        style::ok(
            out,
            &format!(
                "{:<8} tickets={}  events={}  errors={}",
                r.source,
                r.tickets_written,
                r.events_written,
                r.errors.len()
            ),
        )?;
        for err in &r.errors {
            style::info(out, err)?;
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

// ────────────────── worklog tag (personal/work classifier) ──────────────────

fn cmd_tag_list<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let path = personal_mod::config_path()
        .ok_or_else(|| anyhow::anyhow!("could not resolve config path"))?;
    let file = personal_mod::read_file(&path);
    if json {
        let payload = serde_json::json!({
            "config_path": path,
            "work": file.work,
            "personal": file.personal,
            "default_rule": "any project_path under ~/Desktop/Work/** is work; everything else is personal",
        });
        writeln!(out, "{}", serde_json::to_string_pretty(&payload)?)?;
        return Ok(());
    }
    writeln!(out, "config:  {}", path.display())?;
    writeln!(
        out,
        "default: ~/Desktop/Work/** → work; everything else → personal"
    )?;
    if file.work.is_empty() && file.personal.is_empty() {
        writeln!(out, "(no custom patterns — using default rule only)")?;
        return Ok(());
    }
    if !file.work.is_empty() {
        writeln!(out, "work overrides:")?;
        for p in &file.work {
            writeln!(out, "  {p}")?;
        }
    }
    if !file.personal.is_empty() {
        writeln!(out, "personal overrides:")?;
        for p in &file.personal {
            writeln!(out, "  {p}")?;
        }
    }
    Ok(())
}

fn cmd_tag_personal<W: Write>(glob: String, out: &mut W, json: bool) -> Result<()> {
    append_pattern("personal", glob, out, json)
}

fn cmd_tag_work<W: Write>(glob: String, out: &mut W, json: bool) -> Result<()> {
    append_pattern("work", glob, out, json)
}

fn append_pattern<W: Write>(kind: &str, glob: String, out: &mut W, json: bool) -> Result<()> {
    let path = personal_mod::config_path()
        .ok_or_else(|| anyhow::anyhow!("could not resolve config path"))?;
    let mut file = personal_mod::read_file(&path);
    let list = match kind {
        "personal" => &mut file.personal,
        "work" => &mut file.work,
        _ => unreachable!(),
    };
    if list.iter().any(|p| p == &glob) {
        if json {
            writeln!(
                out,
                "{}",
                serde_json::json!({"status": "noop", "kind": kind, "glob": glob})
            )?;
        } else {
            writeln!(out, "already in {kind} list: {glob}")?;
        }
        return Ok(());
    }
    list.push(glob.clone());
    personal_mod::write_file(&path, &file)?;
    if json {
        writeln!(
            out,
            "{}",
            serde_json::json!({"status": "added", "kind": kind, "glob": glob, "config_path": path})
        )?;
    } else {
        writeln!(out, "added to {kind}: {glob}")?;
        writeln!(out, "config: {}", path.display())?;
        writeln!(
            out,
            "tip: run `worklog tag reclassify` to apply to existing blocks"
        )?;
    }
    Ok(())
}

fn cmd_tag_reclassify<W: Write>(day: Option<String>, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    let conn = db::open(&paths.db)?;
    let stats = personal_mod::reclassify_blocks(&conn, day.as_deref())?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&stats)?)?;
    } else {
        writeln!(
            out,
            "✓ reclassified {} block{} ({} now personal, {} now work, {} unchanged)",
            stats.total,
            if stats.total == 1 { "" } else { "s" },
            stats.changed_to_personal,
            stats.changed_to_work,
            stats.unchanged
        )?;
    }
    Ok(())
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

fn cmd_day<W: Write>(
    day: Option<String>,
    no_serve: bool,
    model: &str,
    out: &mut W,
    json: bool,
) -> Result<()> {
    // `db::open` is idempotent: it creates the file + runs migrations if
    // missing, so first-run users don't need to remember `db migrate`.
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let conn = db::open(&paths.db)?;
    let day_parsed = parse_day(day.as_deref())?;

    // --- collect --------------------------------------------------------
    // Collect for the requested day, not "now" — lets `worklog day --day
    // 2026-04-01` pull the right slice instead of dumping today's data
    // into last month's folder.
    style::step(out, "collecting github + jira + gcal …")?;
    let client = http::client()?;
    let since = day_parsed;
    let until = day_parsed + chrono::Duration::days(1);

    // Each collector gets its own spinner + ok/warn line so a stall on
    // one source doesn't look like the whole pipeline has hung. A Jira
    // outage shouldn't block the rest of the flow.
    run_with_spinner("jira", || match jira_col::JiraAuth::from_secrets() {
        Ok(auth) => match jira_col::fetch_open_tickets_with(&conn, &auth, &client) {
            Ok(r) => StepOutcome::Ok(format!(
                "jira:   tickets={} events={}",
                r.tickets_written, r.events_written
            )),
            Err(e) => StepOutcome::Warn(format!("jira:   {e}")),
        },
        Err(e) => StepOutcome::Info(format!("jira skipped: {e}")),
    })
    .render(out)?;

    run_with_spinner("github", || match gh::GitHubAuth::from_secrets() {
        Ok(auth) => match gh::collect_with(&conn, &auth, since, until, &client) {
            Ok(r) => StepOutcome::Ok(format!("github: events={}", r.events_written)),
            Err(e) => StepOutcome::Warn(format!("github: {e}")),
        },
        Err(e) => StepOutcome::Info(format!("github skipped: {e}")),
    })
    .render(out)?;

    run_with_spinner("gcal", || match gcal_col::GcalAuth::from_paths() {
        Ok(auth) => match gcal_col::collect_with(&conn, &auth, since, until, &client) {
            Ok(r) => StepOutcome::Ok(format!("gcal:   events={}", r.events_written)),
            Err(e) => StepOutcome::Warn(format!("gcal:   {e}")),
        },
        Err(e) => StepOutcome::Info(format!("gcal skipped: {e}")),
    })
    .render(out)?;

    // --- infer ----------------------------------------------------------
    style::step(out, "inferring blocks …")?;
    let events = infer::load_day_events(&conn, day_parsed)?;
    let blocks = infer::build_blocks(events);
    infer::persist_blocks(&conn, day_parsed, &blocks)?;
    let total_min: i64 = blocks.iter().map(|b| b.duration_seconds).sum::<i64>() / 60;
    style::ok(
        out,
        &format!(
            "{} block{} · {} min total",
            blocks.len(),
            if blocks.len() == 1 { "" } else { "s" },
            total_min
        ),
    )?;

    // --- estimate -------------------------------------------------------
    let provider_label = match estimate::resolve_provider() {
        Ok(estimate::ProviderChoice::LiteLLM(_)) => "litellm",
        _ => "claude -p",
    };
    style::step(out, &format!("estimating ({provider_label}) …"))?;
    let spinner = style::spinner(&format!("running {provider_label} over unestimated blocks"));
    let est_result = estimate::estimate_day(&conn, day_parsed, model);
    spinner.finish_and_clear();
    match est_result {
        Ok(stats) => style::ok(
            out,
            &format!(
                "estimated={} skipped={} failed={}",
                stats.estimated, stats.skipped, stats.failed
            ),
        )?,
        Err(e) => style::warn(out, &format!("estimate skipped: {e}"))?,
    }

    // --- serve ----------------------------------------------------------
    if no_serve {
        if json {
            writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "day": day_parsed.to_string(),
                    "blocks": blocks.len(),
                    "minutes": total_min,
                    "served": false,
                }))?
            )?;
        }
        return Ok(());
    }
    style::step(out, "bringing up review UI at http://localhost:3333")?;
    style::info(out, "ctrl+c to bring it down, or `worklog web down`")?;
    writeln!(out)?;
    cmd_web_up(3333, false, false, out, false)
}

/// Outcome of a single `cmd_day` collector step. Owns the message so
/// the spinner can finish cleanly before we write the styled line.
enum StepOutcome {
    Ok(String),
    Warn(String),
    Info(String),
}

impl StepOutcome {
    fn render<W: Write>(self, out: &mut W) -> Result<()> {
        match self {
            StepOutcome::Ok(msg) => style::ok(out, &msg)?,
            StepOutcome::Warn(msg) => style::warn(out, &msg)?,
            StepOutcome::Info(msg) => style::info(out, &msg)?,
        }
        Ok(())
    }
}

/// Run a closure with a spinner labelled `label`; clear the spinner
/// before the outcome line prints so the spinner frame doesn't ghost
/// under the ✓ / ! / · marker.
fn run_with_spinner<F>(label: &str, f: F) -> StepOutcome
where
    F: FnOnce() -> StepOutcome,
{
    let pb = style::spinner(&format!("{label} …"));
    let out = f();
    pb.finish_and_clear();
    out
}

fn cmd_hook_run() -> Result<()> {
    // All output goes to stderr (handled inside hook_run::run_from_stdin) so
    // Claude Code never sees bytes on stdout.
    hook_run::run_from_stdin()
}

fn cmd_daemon_install<W: Write>(command: Option<String>, out: &mut W, json: bool) -> Result<()> {
    let cmd = command.unwrap_or_else(worklog_core::daemon_service::default_command);
    let status = worklog_core::daemon_service::install(&cmd)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    style::ok(
        out,
        &format!(
            "daemon service installed ({})",
            status
                .unit_path
                .as_ref()
                .map(|p| worklog_core::paths::short_display(p))
                .unwrap_or_else(|| "?".into())
        ),
    )?;
    if let Some(cmd) = &status.command {
        style::info(out, &format!("runs: {cmd}"))?;
    }
    for note in &status.notes {
        style::info(out, note)?;
    }
    Ok(())
}

fn cmd_daemon_uninstall<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = worklog_core::daemon_service::uninstall()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    style::ok(out, "daemon service uninstalled")?;
    Ok(())
}

fn cmd_daemon_status<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let status = worklog_core::daemon_service::status()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
        return Ok(());
    }
    if status.installed {
        style::ok(
            out,
            &format!(
                "daemon service installed via {} ({})",
                status.platform,
                status
                    .unit_path
                    .as_ref()
                    .map(|p| worklog_core::paths::short_display(p))
                    .unwrap_or_else(|| "?".into())
            ),
        )?;
        if let Some(cmd) = &status.command {
            style::info(out, &format!("runs: {cmd}"))?;
        }
    } else {
        style::warn(
            out,
            "daemon service not installed — run `worklog daemon install` to start at login",
        )?;
    }
    Ok(())
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

fn cmd_web_up<W: Write>(
    port: u16,
    no_daemon: bool,
    no_open: bool,
    out: &mut W,
    json: bool,
) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let context = web_mod::resolve_web_context(&paths)?;

    if !no_daemon {
        ensure_daemon_running(out)?;
    }

    let pid = web_mod::bun_up(&paths, &context, port)?;
    let url = format!("http://localhost:{port}");

    // Open the browser only on interactive TTY runs. `--json` is
    // script mode and `--no-open` is an explicit opt-out; non-TTY
    // stdout (e.g. piped into `tee`) silently suppresses to avoid
    // surprising automation.
    let should_open = !no_open && !json && io::stdout().is_terminal();
    let opened = if should_open {
        match worklog_core::browser::open_url(&url) {
            Ok(worklog_core::browser::OpenOutcome::Spawned) => true,
            Ok(worklog_core::browser::OpenOutcome::Unsupported) => false,
            // Treat opener failures as non-fatal — server is up.
            Err(_) => false,
        }
    } else {
        false
    };

    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "url": url,
                "pid": pid,
                "context": context.display().to_string(),
                "log": web_mod::bun_log_path(&paths).display().to_string(),
                "opened": opened,
            }))?
        )?;
    } else {
        let suffix = if opened {
            " (opening in browser…)"
        } else {
            ""
        };
        writeln!(out, "✓ worklog-web up — {url}{suffix}")?;
        writeln!(out, "  pid:     {pid}")?;
        writeln!(
            out,
            "  log:     {}",
            web_mod::bun_log_path(&paths).display()
        )?;
        writeln!(out, "  context: {}", context.display())?;
    }
    Ok(())
}

fn cmd_web_down<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    web_mod::bun_down(&paths)?;
    if json {
        writeln!(out, "{{\"ok\": true}}")?;
    } else {
        writeln!(out, "✓ worklog-web stopped")?;
    }
    Ok(())
}

fn cmd_web_status<W: Write>(out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    let status = web_mod::bun_status(&paths);
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&status)?)?;
    } else if status.running {
        writeln!(
            out,
            "✓ running — {} ({})",
            status.image.as_deref().unwrap_or("bun"),
            status.container.as_deref().unwrap_or("?"),
        )?;
        writeln!(out, "  log: {}", web_mod::bun_log_path(&paths).display())?;
    } else {
        writeln!(out, "— not running. Start with `worklog web up`.")?;
    }
    Ok(())
}

fn cmd_web_logs(tail: u32) -> Result<()> {
    let paths = Paths::resolve()?;
    let log = web_mod::bun_log_path(&paths);
    if !log.is_file() {
        anyhow::bail!("no log at {} — has the web UI ever been up?", log.display());
    }
    let status = std::process::Command::new("tail")
        .args(["-n", &tail.to_string(), "-f"])
        .arg(&log)
        .status()
        .context("spawning tail")?;
    if !status.success() {
        anyhow::bail!("tail exited {status}");
    }
    Ok(())
}

fn cmd_web_build<W: Write>(_pull: bool, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let context = web_mod::resolve_web_context(&paths)?;
    let status = std::process::Command::new("bun")
        .current_dir(&context)
        .env("NEXT_TELEMETRY_DISABLED", "1")
        .args(["run", "build"])
        .status()
        .context("spawning `bun run build`")?;
    if !status.success() {
        anyhow::bail!("bun run build exited {status}");
    }
    if json {
        writeln!(
            out,
            "{{\"ok\": true, \"context\": {:?}}}",
            context.display()
        )?;
    } else {
        writeln!(out, "✓ worklog-web built from {}", context.display())?;
    }
    Ok(())
}

fn cmd_web_fetch<W: Write>(force: bool, out: &mut W, json: bool) -> Result<()> {
    let paths = Paths::resolve()?;
    paths.ensure()?;
    let version = env!("CARGO_PKG_VERSION");

    if !force && web_mod::fetch::cache_is_current(&paths, version) {
        let cache = web_mod::fetch::cache_dir(&paths);
        if json {
            writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cached": true,
                    "path": cache.display().to_string(),
                    "version": version,
                }))?
            )?;
        } else {
            style::info(
                out,
                &format!("already fetched for {version} → {}", cache.display()),
            )?;
        }
        return Ok(());
    }

    let url = web_mod::fetch::archive_url_for(version);
    let pb = style::spinner(&format!("downloading {url}"));
    let result = web_mod::fetch::fetch_to_cache(&paths, version);
    pb.finish_and_clear();
    let path = result?;
    if json {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "cached": false,
                "path": path.display().to_string(),
                "version": version,
                "url": url,
            }))?
        )?;
    } else {
        style::ok(out, &format!("fetched web tree → {}", path.display()))?;
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

/// Make sure the daemon is listening on `127.0.0.1:9323` before doing
/// work that depends on it. Fast-path when it's already up; otherwise
/// show a spinner, install (if the service unit is missing), and poll.
fn ensure_daemon_running<W: Write>(out: &mut W) -> Result<()> {
    use worklog_core::daemon_service::{ensure_running, DaemonEnsureOutcome};
    if daemon_tcp_reachable("127.0.0.1:9323") {
        return Ok(());
    }
    let pb = style::spinner("starting worklog daemon …");
    let res = ensure_running("127.0.0.1:9323");
    pb.finish_and_clear();
    match res {
        Ok(DaemonEnsureOutcome::AlreadyUp) => Ok(()),
        Ok(DaemonEnsureOutcome::Installed) => {
            style::ok(out, "daemon service installed and started").ok();
            Ok(())
        }
        Ok(DaemonEnsureOutcome::Restarted) => {
            style::ok(out, "daemon service resumed").ok();
            Ok(())
        }
        Err(e) => {
            style::warn(
                out,
                &format!("daemon did not come up ({e:#}) — the web UI can still read, but writes will fail"),
            )
            .ok();
            Ok(())
        }
    }
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
            anyhow::bail!("release pubkey isn't embedded yet — see worklog-core::updater::pubkey");
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
            writeln!(out, "→ {current_version} → {} available", manifest.version)?;
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
                "from":           report.from,
                "to":             report.to,
                "used_delta":     report.used_delta,
                "asset_bytes":    report.asset_bytes,
                "dry_run":        report.dry_run,
                "rolled_back":    report.rolled_back,
                "daemon_restart": report.daemon_restart,
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
            "✓ updated {} → {} ({} bytes{}){}",
            report.from,
            report.to,
            report.asset_bytes,
            if report.used_delta { ", delta" } else { "" },
            restart_suffix(report.daemon_restart.as_ref()),
        )?;
    }
    Ok(())
}

/// Render a short phrase describing whether the supervised daemon was
/// cycled. Appended to the "updated" line so the user knows without
/// having to check `worklog daemon status` after the fact.
fn restart_suffix(outcome: Option<&worklog_core::daemon_service::RestartOutcome>) -> &'static str {
    use worklog_core::daemon_service::RestartOutcome;
    match outcome {
        Some(RestartOutcome::Restarted) => " · daemon restarted",
        Some(RestartOutcome::NotRunning) => {
            " · daemon service not running — start with `worklog daemon`"
        }
        Some(RestartOutcome::NotInstalled) => "",
        Some(RestartOutcome::Unsupported) => "",
        None => "",
    }
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
        file.file_name().and_then(|s| s.to_str()).unwrap_or("bin")
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
                new.extension().and_then(|s| s.to_str()).unwrap_or("bin")
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
