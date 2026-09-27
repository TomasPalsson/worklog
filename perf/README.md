# worklog performance + space evals

Target: this Mac only (latest macOS, Apple Silicon M5 Pro, 15 cores). Base: PR #54
tip `26742ac` (schema v15) — the build that actually runs here.

Every optimisation must pass BOTH:
- **timing eval** — `bun perf/bench.ts` median/p95 per scenario vs the baseline binary;
- **behaviour eval** — the same scenario run by the baseline binary and the candidate
  on identical fixture copies must produce identical observable output (stdout, HTTP
  bodies, DB rows), with only wall-clock fields masked. Plus `cargo test` and `bun test`.

Fixtures are local only (built from the owner's real data, never committed):
`perf/fixture.sh` clones the DB snapshot and a frozen `~/.claude/projects` subset into
`$PERF_DIR` (APFS clones, ~0 extra disk). Runs are hermetic: `WORKLOG_HOME`, `HOME`,
`WORKLOG_SECRETS_FILE`, `WORKLOG_ESTIMATOR_PROVIDER=litellm` (no URL → no paid calls),
`WORKLOG_PRUNE_ENABLED=false`, private daemon on `127.0.0.1:19323` + private socket.

## Scenarios (user stories)

| id | As the owner, I… | what runs | metric |
|----|------------------|-----------|--------|
| T1 hook | start/end a Claude Code session | `worklog hook-run` (stdin JSON) | ms / call |
| T2 tick-warm | let the 15-min job run while I work | `worklog day --day 2026-09-25` on a DB that already has the day | s |
| T3 tick-cold | run the job for a day not yet ingested | same, on a fresh DB | s |
| T4 day-page | open a day in the review UI | daemon `/days/D`, `/days/D/routed`, `/billing/registry`, `/tickets` in parallel (D=2026-09-22) | ms |
| T5 week-page | open the week view | 7 × `/days/*` for Mon 2026-09-21 | ms |
| T6 block-details | open the heaviest block's Details | `/blocks/:id/details`, `/blocks/:id/events` | ms |
| T7 billing-tab | switch the day to billing view | `/export/2026-09-22` | ms |
| T8 cli-reads | use the CLI | `summary`, `week`, `block list`, `infer --day` | ms |
| T9 purge | monthly prune | `db purge` on a year-scaled copy | s |
| T10 decide | let the decision model sort browser/Slack events | 90 pending events of 2026-09-24 → classifier, cold and repeat tick | s |
| T11 describe | get block + billing-line descriptions written | `claude -p` per block/line (paid, opt-in `--paid`) | s/block, $/block |
| S1 db-week | keep a week of data | DB bytes after checkpoint | MB |
| S2 per-event | ingest a day | bytes / event on fresh ingest | B |
| S3 footprint | have worklog installed | model dir + WAL + backups | MB |

Behaviour oracle for T10 = identical `(event, choice, probability, runner_up, abstain)` per
event (the model is deterministic: 0/20 repeat mismatches). T11 is non-deterministic, so its
performance eval is a blind LLM judge: candidate descriptions must be rated "as good or
better" than baseline on ≥ 95% of blocks, valid against the schema, and never leak
persona text (e.g. "Mr Claude").

## Baseline (first probes, before the harness; harness numbers replace these)

| id | today | goal |
|----|-------|------|
| T1 hook | 15.3 ms (floor: tiny Rust exec 3.2 ms) | ≤ 2 ms; 0 ms perceived with `async: true` |
| T8 week | 154 ms | ≤ 10 ms |
| T8 summary / block list / infer | 12 / 17 / 26 ms | ≤ 3 ms each |
| T10 decide | 105 events (63 unique), 44 options: 42.1 s cold, 45.6 s repeat tick (~430 ms/event, 4 ORT threads, CPU) | cold ≤ 4 s; repeat tick ≤ 50 ms |
| T11 describe | real prompts (22 blocks of 2026-09-22, ~4 KB each): 20.4 s and $0.044 per block, sequential → ~7.5 min/day; ~2k hidden thinking tokens for an ~80-char answer | ≤ 2 s/block effective, ≤ $0.003/block, judge parity |
| S1 db-week | 31.9 MB (raw_json 16.5 MB of it) | ≤ 8 MB |
| S3 footprint | model dir 1.2 GB (onnx + safetensors both loaded) | measure first |

## Findings (probes, 2026-09-27)

- T11, same 8 captured prompts (`perf/describe.ts`): **base** 20.4 s/block, $0.044;
  **lean** (`--tools "" --strict-mcp-config --setting-sources "" --disable-slash-commands
  --no-session-persistence`) 19.1 s, $0.012; **fast** (lean + `MAX_THINKING_TOKENS=0`)
  6.8 s, $0.005. Today every call also loads the owner's CLAUDE.md/plugins/hooks into the
  estimator. Blind judge (8 blocks, 4 unlabeled answers each, scores /20): base 15.6,
  base-again 15.8 (noise), lean 14.3, fast 13.6 (0 "best", 2 "unacceptable" — invents
  work, e.g. "pharmacy review flow"). **fast rejected** (fails quality); **lean not
  adopted** (trend worse, n=8 too small). Quality-neutral levers instead: run blocks
  concurrently (same prompt + flags) and skip blocks whose prompt is unchanged.
- T10: 10 ORT threads measured 84 s vs 42 s at 4 — but the machine was busy; re-measure quiet.
- T1: hook runs only on SessionStart/SessionEnd, in parallel with slower hooks → low impact.
  `schema.sql` re-apply costs ~2 ms per CLI start; the repo convention relies on it running
  every open (tables are added without a version bump), so a skip must key on a schema hash.
- Space: `verdict-model/` holds model.onnx (inference) + model.safetensors (rlcd also loads a
  PyTorch copy as fallback) = 1.2 GB; ~600 MB of old DB backups sit in the data dir.

## Coordination

PR #54's session (`prep-block-clues` worktree) is live in `infer*`, `estimate.rs`,
`line_text*`, `clues_*`, `daemon.rs`, `billing*`, `collectors/*` and some web components,
and will merge #54 into main later today. Changes to those files wait for that merge, land
as small separate commits, and are announced to that session first. Never deploy to
`~/.local/bin`, never touch `~/.local/share/worklog`, never bind 9323/9324/3333.

## Change log

| # | change | scenario | before → after | behaviour eval |
|---|--------|----------|----------------|----------------|
| 1 | verdict server: `lru_cache` on the model call (deterministic model) | T10 | cold 42.1 s → 24.7 s (1.7×); repeat tick 45.6 s → 0.024 s (~1900×) | 105/105 answers byte-identical to recorded golden; `cargo test verdict` 6/6; `--self-test` OK |
