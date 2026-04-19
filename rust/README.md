# worklog/rust — the Rust workspace

The `worklog` binary. Everything user-facing ships from here:
collectors, infer, estimator, tempo sync, axum daemon, web
orchestration, signed self-updater, release tooling.

## Layout

```
rust/
├── Cargo.toml                      # workspace
├── Justfile                        # just check | fmt | test | demo | release
└── crates/
    ├── worklog-core/               # shared lib
    │   ├── sql/schema.sql          # canonical schema (embedded via include_str!)
    │   ├── templates/docker-compose.yml  # rendered by `worklog web up`
    │   └── src/
    │       ├── lib.rs paths.rs db.rs models.rs repo.rs secrets.rs
    │       ├── hook.rs schedule.rs http.rs tz.rs
    │       ├── infer.rs sessions.rs hook_run.rs estimate.rs
    │       ├── block_service.rs daemon.rs web.rs
    │       ├── collectors/{gcal,github,jira,tempo}.rs
    │       └── updater/{mod,crypto,delta,fetch,install,manifest,pubkey,signing}.rs
    └── worklog-cli/                # the `worklog` binary
        ├── src/{main,lib,cli,wizard}.rs
        └── tests/{cli,daemon}.rs   # assert_cmd end-to-end tests
```

## Commands

Run everything from the repo root unless otherwise noted.

```bash
cargo test  --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt   --manifest-path rust/Cargo.toml --all -- --check
cargo build --release --manifest-path rust/Cargo.toml --bin worklog
```

Or use the `Justfile`:

```bash
cd rust
just check                          # fmt + clippy + test
just test
just demo                           # run the wizard into /tmp/worklog-demo
```

## Release pipeline

`.github/workflows/release.yml` triggers on `v*` tag push:

1. Native builds on `macos-14` (aarch64-apple-darwin) and
   `ubuntu-24.04` (x86_64-unknown-linux-gnu).
2. Strip binary, zstd-compress for the `full` asset, sign with
   `worklog dev sign` using the private key from
   `WORKLOG_RELEASE_PRIVATE_KEY` GHA secret.
3. Assemble a top-level `manifest.json` covering every target; sign
   it. Shred the key.
4. `gh release create` publishes eight files per release.

`install.sh` (at the repo root) is the bootstrap for new installs;
`worklog upgrade` drives subsequent signed updates.

Local pre-tag smoke:

```bash
bash scripts/release-smoke.sh   # host-side dry run of assemble + sign
bash tests/install/smoke.sh     # bash-level asserts on install.sh
```

## Conventions

- Collectors are blocking (single-shot CLI invocations); the daemon
  wraps them via `tokio::task::spawn_blocking`.
- Every collector exposes both `collect(...)` and a test-injectable
  `collect_with(..., client)`; tests use httpmock fixtures.
- Dedupe is always via `repo::upsert_event`'s `UNIQUE(source,
  source_id)` — collectors never read-then-write.
- Secrets via `secrets::{get, require}` → OS keychain with
  `WORKLOG_SECRETS_FILE` env-file fallback for tests.

See [`../CLAUDE.md`](../CLAUDE.md) for project-level invariants.
