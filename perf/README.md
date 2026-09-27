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

## Baseline and goals

Baseline = `bun perf/bench.ts --bin worklog-base` (PR #54 @ `26742ac`), medians, default
runs, 2026-09-27. CLI runs start on a fresh clone, so the DB file is cold in the page cache
(same for base and candidate). T10/T11 from `perf/decide.ts` / `perf/describe.ts`.

| id | today | goal | speedup asked |
|----|-------|------|---------------|
| T1 hook | 17.2 ms (floor: tiny Rust exec ≈ 3 ms) | ≤ 3 ms; 0 ms perceived via `async: true` | 6× / ∞ |
| T2 tick-warm | 1418 ms | ≤ 30 ms | 47× |
| T3 tick-cold | 1359 ms | ≤ 300 ms | 4.5× |
| T4 day-page | 28.6 ms | ≤ 3 ms | 10× |
| T5 week-page | 59.1 ms | ≤ 5 ms | 12× |
| T6 block-details | 0.7 ms | ≤ 0.7 ms (don't regress) | 1× |
| T7 billing-tab | 1.3 ms | ≤ 1 ms | 1.3× |
| T8 summary / week / block list / infer | 29.8 / 40.0 / 28.1 / 66.0 ms | ≤ 5 / 6 / 5 / 10 ms | 6× |
| T9 purge | 896 ms | ≤ 100 ms | 9× |
| T10 decide | 105 events (63 unique): cold 42.1 s, repeat tick 45.6 s | cold ≤ 4 s; repeat ≤ 50 ms | 10× / 900× |
| T11 describe | 20.4 s + $0.044 per block, sequential → ~7.5 min for 22 blocks | ≤ 1 min/day at judge parity and same cost | 7.5× |
| S1 db-week | 31.9 MB (raw_json 15.8 MB) | ≤ 8 MB | 4× |
| S3 footprint | model dir 1.2 GB; ~600 MB old DB backups | ≤ 0.6 GB model (owner decision) | 2× |

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
- T2 profile (fresh clones, D=2026-09-25): whole tick 1.4–1.8 s, of which `collect
  transcripts` 1.4–1.9 s; shell 37 ms, reflog 18 ms, infer 51 ms. The transcript collector
  re-reads + re-parses every file touched since midnight on every tick.
- T4: `/billing/registry` costs ~16 ms warm / ~160 ms cold because `unmapped_folders` reads
  30 days of fat `events` rows for `project_path`; a covering index (started_at,
  project_path) measured 17 → 3 ms warm, 213 → 5 ms cold (+1.6 MB). Fat rows (raw_json
  inline) slow every events scan — candidate: move raw_json to a side table.
- T10 threads, quiet machine: 4 threads 41.8 s, 5 threads 42.4 s (10 threads 84 s, busy
  machine). 4 is the sweet spot; inference is compute-bound (~400 ms/event).
- T10 concurrency probe (3 engines + ThreadingHTTPServer + 3 concurrent clients): 36.4 s vs
  24.7 s single-engine cached — **worse** (cores contend; efficiency cores drag). Rejected.
- Idea sweep (`perf-ideas` workflow: 4 independent generators + adversarial critics).
  Survived: share `load_day_events` across day_activity/day_overlaps; bound Details SQL to
  the block span; cache day summaries until the DB changes; purge datetime() fix;
  parallel `claude -p`; concurrent `/classify` (then refuted by the probe above); batched
  forward pass (unproven). Killed: WAL read pool (breaks snapshot consistency), hook via
  daemon, tick inside daemon, version-gated migrate (breaks the re-apply convention),
  owner_runs two-pointer (changes latest_human semantics), billing haystack de-dup
  (negligible), raw_json dictionary compression (reader audit incomplete), prompt-cache
  warm-up and one long `claude` session (negligible / cross-block contamination), skipping
  the PyTorch fallback load (needs re-implementing rlcd calibration).
- T1: hook runs only on SessionStart/SessionEnd, in parallel with slower hooks → low impact.
  `schema.sql` re-apply costs ~2 ms per CLI start; the repo convention relies on it running
  every open (tables are added without a version bump), so a skip must key on a schema hash.
- Space: `verdict-model/` holds model.onnx (inference) + model.safetensors (rlcd also loads a
  PyTorch copy as fallback) = 1.2 GB; ~600 MB of old DB backups sit in the data dir.

## Next (resume here)

Rebased on PR #54 @ `88d1d77` (it now runs `claude -p` with `--tools "" --strict-mcp-config
--setting-sources project` itself). New baseline binary: `worklog-base2` built from HEAD.
1. T4/T5: load the day's events once in `stitch_day_summary`; then cache each day summary
   keyed by `PRAGMA data_version` + `total_changes()` (only DB inputs — audit first).
2. T4: covering index `events(started_at, project_path)` for `unmapped_folders`.
3. T2: skip transcript files unchanged since the last tick (keep resumed-session dedupe
   semantics; oracle = tick on truncated fixture, then on full fixture).
4. T11: run `claude -p` calls concurrently, commit in original order.
5. T6: pre-filter Details SQL by the block's calendar day(s).
6. T9: purge prefilter `started_at < date(cutoff,'+2 days') AND datetime(...) < datetime(...)`.
7. S1: raw_json compression — plain zstd 1.9×, dictionary 3.1× (dictionary must not be
   committed: trained on private data → store it in the DB).

Owner decisions pending: `async: true` on the worklog hook; delete ~600 MB of old DB
backups; int8/fp16 model (changes numerics); trimming never-displayed tool input.

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
| 2 | index `events(project_path, started_at)` for `unmapped_folders` | T4 | 23.0 → 12.2 ms (1.9×); query 17 → 1.2 ms warm; +1.7 MB/week | all 12 scenarios PASS / PASS* (only the new SCHEMA line and purge's "bytes freed"); other plans unchanged |
| 3 | upsert `DO UPDATE … WHERE` a value changes | T2 writes | WAL per steady tick 78.2 → 12.9 MB (6×); time unchanged (parsing-bound) | T1/T2/T3/T8d/T9 PASS; new failing-first unit test; 983 tests pass |

## How to run

```bash
# build the fixture once (real data, never committed)
PERF_DIR=/path/to/perf-fixture bash perf/fixture.sh

# prove the oracle has teeth before trusting any numbers
PERF_DIR=/path/to/perf-fixture bun perf/bench.ts --selftest --base /path/to/worklog-base

# candidate vs baseline, all scenarios
PERF_DIR=/path/to/perf-fixture bun perf/bench.ts --bin /path/to/worklog-candidate \
  --base /path/to/worklog-base --json perf-results.json

# a subset, with an explicit run count
PERF_DIR=/path/to/perf-fixture bun perf/bench.ts --bin /path/to/worklog \
  --only T1,T4 --runs 10
```

No `--base` → prints each scenario's masked-oracle sha256 instead of a PASS/FAIL.
On a FAIL, full oracles are saved to `$PERF_DIR/runs/oracle-<id>-{base,cand}.txt`.
