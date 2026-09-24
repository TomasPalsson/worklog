Task list: `.claude/loop/tasks.json` (items L1–L5). Background, read once: `.specs/USER-STORIES.md`
(S3, S4, S7, S8, S9) and `.specs/DATA-SOURCES.md`. Repo rules: `CLAUDE.md` (collectors must be
idempotent via `repo::upsert_event`; never print to stdout from `worklog hook-run`; UTC in the DB).

Pick the FIRST item whose `passes` is false. Do only that item:
1. Write its tests first and run its `verify` command — confirm it fails.
2. Write the minimum code that makes it pass. Touch only the item's `files`.
3. Run its `verify` again, then `cargo test --manifest-path rust/Cargo.toml` and
   `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` and
   `cargo fmt --manifest-path rust/Cargo.toml --all` — all must be clean.
4. Set that item's `passes` to true only after its own `verify` exited 0, commit with a message that
   describes the change (never the item id alone), append one dated line to
   `.claude/loop/LEARNINGS.md`, and stop.

Web items (L7): `web/` is Next.js + Bun; run `cd web && bun test && bun run typecheck`; never
run `bun run build`. The day data comes from the daemon's `GET /days/:day` (`web/lib/daemon.ts`).
A size-guard hook complains that daemon.rs is over 400 lines — it was ~4300 before this loop;
make the minimal change there and do not split the file.

Privacy is part of every item: never store shell command text, commit message bodies, or anything
outside the owner's machine. Never run the `git` CLI against other repos — read `.git/logs/HEAD`
files directly. Never edit an existing test's assertion to make it pass.
