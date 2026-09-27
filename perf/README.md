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

State 2026-09-27 ~16:00: PR #54 merged to main (`d2587fe`); `perf/evals` rebased on it.
Done on the branch: #1–#6 in the change log. Baselines: `worklog-base3` = #3.
Slice branches (built in isolated worktrees `.claude/worktrees/wf_a517011d-04f-{1..5}`):
- `perf/s-ticks`: reviewed BLOCK (cache survived a purge; missing-file uuid-ownership gap;
  unbounded growth). Re-ported + fixed on `perf/r2-ticks` (#7 in the change log) — ported
  onto `perf/evals` tip via a 3-way patch, both findings fixed, retention added.
- `perf/s-describe`: review said FIX — (1) estimate phase-3 UPDATE must skip rows that
  became `manual` meanwhile; (2) its line_text half has no production caller (the daemon
  uses `daemon_line_text::generate_day`) → drop it, parallelise that path instead with
  invoke outside the DB lock.
Then: tool-input trim in `collectors/claude_tools.rs` (owner OK'd; keep the 9 preview keys
file_path/notebook_path/path/command/pattern/description/prompt/url/query verbatim; for
default-case tools cap each string at 200 chars); raw_json compression (plain zstd 1.9×;
a dictionary must not be committed — trained on private data); one PR to main.

Idea sweep 2 (fresh generators + critics), queued for round 3 after round 2 merges:
T3 cold tick — parse each transcript line once (claude_tools::collect_tool_outputs re-parses
every line), one transaction per collect run, `RETURNING id` (fallback SELECT when the
no-op-guarded upsert returns nothing); T4/T5 — fold stitch_day_summary's 3 enrichment
queries into one fetch + prepare_cached. Owner decision: tool output cap 2048 → 512 B
(~3 MB/week; Details shows less). Killed: day-summary cache (key must also cover TZ/non-DB
inputs), raw_json side table (missed readers), schema-hash open (<5% after the hook change),
page size 16 KB (4× WAL bytes per small commit), dropping jira/tempo indexes (used).

Owner decisions: backups deleted (~590 MB, 006 ones after #54 merged); hook made
non-blocking in the flow repo (`plugins/flow/hooks/worklog-hook.sh`: detached `worklog
hook-run`, 99/99 hook tests, not committed there — owner's repo); tool-input trim approved.
Open: int8/fp16 model (changes numerics); stable block IDs when a day's blocks are unchanged
(would cut the remaining 12.9 MB/tick of writes; changes visible IDs).
Lesson: workflow agents inherit this worktree's sandbox — builders AND reviewers need
`isolation: 'worktree'` (a reviewer's checkout detached this branch once).

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
| 4 | purge: `started_at < cutoff+2d` index pre-filter ahead of the unchanged `datetime()` predicate | T9 | 641 → 651 ms (noise: the two whole-DB VACUUMs dominate at 1 week); SCAN → SEARCH for months of data | reviewer SHIP: sabotage-tested, ~3.3k synthetic timestamps 0 divergences |
| 5 | `stitch_day_summary` loads the day's events once | T4, T5 | T4 8.8 → 6.4 ms; T5 21.8 → 13.7 ms | all 13 scenarios PASS (base = #3, perf2 fixture); reviewer PASS |
| 6 | Details: SQL day pre-filter (block span ± 1 day) | T6b | 8.6 → 8.6 ms (fixture's heaviest session sits in one day; helps multi-day sessions) | T6/T6b PASS; reviewer PASS |
| 7 | transcript_file_cache: skip byte-identical .jsonl on repeat ticks; purge clears it; missing file resets the window; ≤ 1 window kept | T2b | steady tick 1320.7 → 50.7 ms (26×); cache ≈ 1 MB | T2/T2b/T2c/T9 PASS* (declared table); round-1 review BLOCK fixed, round-2 SHIP |
| 8 | `claude -p` block estimates + billing-line texts 4 at a time, applied in original order; estimate never overwrites a block that turned `manual` mid-batch | T11 | fake 200 ms invoker: 6 calls 1.23 s → 0.41 s (≈ 3.5× for a 22-block day) | order + manual-race unit tests; reviewer SHIP |
| 9 | trim claude_tool input to what toolPreview shows | S1 | fresh 4-day ingest 12.91 → 12.16 MB (with #7's ~1 MB table) | toolPreview identical for all 2,241 tool rows (perf/trim.test.ts); reviewer SHIP |
| 7 | `transcript_file_cache` (perf/s-ticks R2): skip byte-identical `.jsonl`s on repeat ticks; fixed the BLOCK review found — purge now clears the cache in the same transaction (else a purged row never came back), a missing cached path invalidates its whole window (cross-file uuid ownership), and retention keeps at most one window | T2, T2b | T2b steady tick 1262 → 49 ms (25.5×); cache table ≈1.0 MB after 4 consecutive day ticks (was 5.1 MB unbounded) | T2/T2b/T2c/T9 PASS* (PERF_IGNORE for the new SCHEMA lines + purge's "bytes freed", both explained by the new table); 3 new unit tests incl. a purge→re-collect repro that fails without the fix; 991 tests pass |

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
