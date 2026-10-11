# Design — Topic stats and search fixes

## 1. Contract files + language

- Rust: `rust/crates/worklog-core/src/topics_contract.rs` (orchestrator-owned; tasks import, never edit).
- Web: `web/lib/topics_contract.ts` (mirror, snake_case, orchestrator-owned).

| canonical | identifier | defined in | banned synonyms |
|---|---|---|---|
| block vector | `Vec<f32>` len `EMBED_DIM`, unit length | topics_contract.rs | embedding_vec, emb |
| vector store | table `block_embeddings` | schema.sql (T005) | vectors, embeddings_cache |
| block text | `embed::block_text(conn, block_id) -> Option<String>` | embed.rs (T005) | doc, content |
| topic | `Topic` | topics_contract.rs | cluster (in JSON/UI), theme |
| topics report | `TopicsReport` | topics_contract.rs | — |
| noise host | `NoiseHost` | topics_contract.rs | spam host |

Times are UTC ISO strings in the DB; `day` is the existing `blocks.day`. Rust is sync (rusqlite + reqwest blocking, as verdict.rs).

## 2. Trust boundaries

| boundary | untrusted shape | parse fn | failure granularity |
|---|---|---|---|
| helper POST /embed reply | JSON | `serde_json` into `EmbedResponse`, then check count == input count and each len == `EMBED_DIM` | whole batch → `Err`, caller maps to `TopicsStatus::HelperDown` |
| `block_embeddings.vec` BLOB | bytes | little-endian f32 × `EMBED_DIM`; wrong length → treat as missing, re-embed | per row |
| `events.details` (firefox URL) | string | `routing::domain_of` | per event; None → skipped |

## 3. Error taxonomy

No new error type. Helper unreachable / non-2xx / bad JSON in `/stats/topics` → `200` with `TopicsReport::empty(HelperDown, ..)`, never a 5xx. DB errors keep the existing daemon `ApiError` mapping.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| helper URL | `routing_contract::CLASSIFIER_ADDR`, overridable by env `WORKLOG_VERDICT_URL` (same as ticket_pick.rs:64) | `embed::EmbedClient::from_env()` | daemon_stats get_topics |
| DB conn | daemon state, as get_stats | `&Connection` | embed, stats_topics |

Schema: `block_embeddings` goes in `schema.sql` (CREATE TABLE IF NOT EXISTS, no version bump — same as `block_undo`, see archive 018 Ruling).

## 6. Deliberately duplicated

- Host parsing: Rust `routing::domain_of` and JS `new URL().hostname` stay separate; both lowercase and keep `www.`. Do not add a shared helper.
- Cosine/normalise math lives only in stats_topics.rs; embed.rs only stores what the helper returns.

## 7. Decisions

- In the context of embeddings, facing a single-threaded helper that already owns the uv env, supervisor and on/off switch, we chose a new `POST /embed` route in `verdict_server.py` with the model loaded lazily on first call, and rejected a second helper process, to reuse the switch and supervisor, accepting that a long embed queues `/classify` (bounded by `EMBED_BATCH`). Makes hard: verdict_server.py, verdict_supervisor.rs uv_args.
- In the context of the Topics section, facing a cold embed of up to 60 s, we chose a separate `GET /stats/topics` route with its own 90 s web timeout, and rejected extending `GET /stats`, so the existing page never waits on or fails with the helper. Makes hard: daemon_stats.rs, web/lib/daemonStats.ts.

## Contract for T001 — Ask matches any word, best first
USAGE      `ask::sync(&conn)?; let hits = ask::search(&conn, "geitin access for vís")?;` → up to 5 hits, best bm25 first.
CONTRACT   none (ask.rs internal).
CALLS      `pub fn to_match_query(query: &str) -> Option<String>` — words quoted as today, joined with " OR ", stopwords dropped unless that leaves none. `search` SQL: `ORDER BY rank, b.started_at DESC, b.id DESC`.
STOPWORDS  `a an the to of in on for and or with by at is are was be from as it this that i my me we our og í á að við fyrir með um sem er var til af` — compare after lowercasing and trimming `.,:;()`.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Noise hosts endpoint
USAGE      `GET /days/2026-10-10/noise-hosts` → `{"hosts":[{"host":"www.youtube.com","dismissed_before":14}]}`
CONTRACT   `topics_contract::{NoiseHost, NoiseHosts, NOISE_MIN_DISMISSED}` — import from it.
CALLS      `pub fn noise_hosts(conn: &Connection, day: &str) -> Result<Vec<NoiseHost>>` in routing_noise.rs. Rule: firefox events with `substr(started_at,1,10) < day` and `label_origin IN ('dismissed','fix','context','link','rule')`; group by `routing::domain_of(details)` (make it `pub(crate)`); keep hosts where every row is `dismissed` and count ≥ `NOISE_MIN_DISMISSED`; sort by count desc, then host.
MODULE     routing_noise.rs may import: rusqlite, anyhow, crate::routing, crate::topics_contract.
THE FIVE   (same five as T001)

## Contract for T003 — Noise tag and bulk dismiss
USAGE      page.tsx: `const noise = await noiseHosts(day).catch(() => ({ hosts: [] }));` → `<UnsortedList ... noiseHosts={noise.hosts} />`
CONTRACT   `web/lib/topics_contract.ts` `NoiseHost`, `NoiseHosts` — import from it.
CALLS      `export async function noiseHosts(day: string): Promise<NoiseHosts>` in web/lib/daemon.ts via `call("GET", \`/days/${day}/noise-hosts\`)`. A group is tagged when `group.source === "firefox"` and `new URL(firstEvent.details).hostname` is in the set. Tag text: `dismissed {n}× before`. Button: `Dismiss {k} noise groups` with the same inline confirm as DismissRest; loops `dismissEvent(id, null, day)` over tagged groups' ids, stops and toasts on the first failure, then `onDismissed(doneIds)`.
THE FIVE   (same five as T001)

## Contract for T004 — Helper /embed
USAGE      `POST http://127.0.0.1:9324/embed {"texts":["fix sso login"]}` → `{"vectors":[[...768 floats...]]}`
CONTRACT   literal values from topics_contract.rs: model `google/embeddinggemma-2`, revision `914f7f89142e33e77833254d9c9b90c3cef7303b`, dim 768.
CALLS      Lazy global, loaded on first /embed: `SentenceTransformer(repo, revision=rev, model_kwargs={"torch_dtype": torch.bfloat16}, config_kwargs={"vision_config": None, "audio_config": None})`. Each text is prefixed `"task: clustering | query: "`. `encode(..., convert_to_tensor=True).float()`, L2-normalise, `.cpu().tolist()`. Empty list or >64 texts → 400. `--self-test` covers /embed with a fake encoder. uv_args gains `--with sentence-transformers>=5 --with torch --with transformers --with pillow --with torchvision` (all five are needed; pillow+torchvision fail at load without them).
THE FIVE   (same five as T001)

## Contract for T005 — Vector store
USAGE      `let client = EmbedClient::from_env(); let vecs: HashMap<i64, Vec<f32>> = embed::ensure_vectors(&conn, &client, &block_ids)?;`
CONTRACT   `topics_contract::{EmbedRequest, EmbedResponse, EMBED_MODEL_REVISION, EMBED_DIM, EMBED_BATCH, PROMPTS_PER_BLOCK, PROMPT_CHARS}`.
CALLS      schema: `CREATE TABLE IF NOT EXISTS block_embeddings (block_id INTEGER PRIMARY KEY REFERENCES blocks(id) ON DELETE CASCADE, model TEXT NOT NULL, text_hash TEXT NOT NULL, vec BLOB NOT NULL);` · `pub fn block_text(conn, block_id: i64) -> Result<Option<String>>` = Jira keys (`\b[A-Z][A-Z0-9]+-\d+\b`) stripped from description + " " + up to PROMPTS_PER_BLOCK prompts (events via block_events, `source='claude' AND title LIKE 'UserPromptSubmit%' AND length(details)>20`, oldest first, each key-stripped and cut to PROMPT_CHARS chars), trimmed; None when empty · `text_hash` = sha256 hex of text · `model` = EMBED_MODEL_REVISION · `ensure_vectors` embeds only rows missing or with a different model/hash, in EMBED_BATCH chunks, upserts, returns vectors for every id that has text · `EmbedClient::embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>` with a 30 s timeout.
THE FIVE   (same five as T001)

## Contract for T006 — Topic math
USAGE      `let report = stats_topics::build(&input);` where `input: TopicsInput { from, to, now: Vec<BlockPoint>, before: Vec<BlockPoint>, history: Vec<BlockPoint>, ticket_summaries: HashMap<String,String> }`
CONTRACT   `topics_contract::*` (report types + MIN_BLOCKS, MIN_TOPICS, MAX_TOPICS, SPRAWL_MIN_BLOCKS, SPRAWL_TOP, SIMILAR_DAYS_TOP).
CALLS      Define in stats_topics.rs: `pub struct BlockPoint { pub id: i64, pub day: String, pub started_at: String, pub seconds: i64, pub ticket: Option<String>, pub description: Option<String>, pub vec: Vec<f32> }`, `pub struct TopicsInput {..}`, `pub fn build(input: &TopicsInput) -> TopicsReport`. Clustering: average-linkage agglomerative on cosine distance, k = clamp(round(sqrt(n/2)), MIN_TOPICS, MAX_TOPICS), ties broken by lower id, deterministic. Drift: assign each `before` point to the nearest topic centroid. Similar days: hour-weighted day centroids over `history` + `now`, compare to day `to` (or the last day in `now` if `to` has none), exclude itself. n < MIN_BLOCKS → `TopicsReport::empty(TooFewBlocks, ..)`.
THE FIVE   (same five as T001)

## Contract for T007 — /stats/topics route
USAGE      `GET /stats/topics?from=2026-10-05&to=2026-10-11` → `TopicsReport` JSON, always 200 unless the DB fails.
CALLS      `pub async fn get_topics(...)` in daemon_stats.rs, beside `get_stats`, same query parsing and defaults. Loads non-personal, non-ignored blocks for [from,to], the previous equal-length range, and 60 days of history before `from`; calls `embed::ensure_vectors` (Err → `HelperDown`, also when `WORKLOG_VERDICT_ENABLED=off`), builds `BlockPoint`s, calls `stats_topics::build`. Blocks with no text count toward `skipped_blocks`. Route `.route("/stats/topics", get(daemon_stats::get_topics))`.
THE FIVE   (same five as T001)

## Contract for T008 — Topics section on /stats
USAGE      page.tsx: `const topics = await getTopics(from, to).catch(() => null);` → `<TopicsSection report={topics} />`
CONTRACT   `web/lib/topics_contract.ts` — import from it.
CALLS      `export async function getTopics(from?: string, to?: string): Promise<TopicsReport>` in web/lib/daemonStats.ts; `timeoutMs` returns 90000 for paths starting `/stats/topics`. `TopicsSection({ report }: { report: TopicsReport | null })`: null or `helper_down` → "Topic stats need the local helper (`worklog verdict`)."; `too_few_blocks` → "Not enough described work in this range yet."; `ready` → five parts: topic bars (name, hours), switches per day with longest run, drift rows (now vs before), catch-all tickets (key, summary, spread), similar days (day, similarity %, top tickets). Wrapped in the page's existing `<Section title="Topics">`. Match the existing stats components' look; reuse RankedBars if it fits.
THE FIVE   (same five as T001)

## Contract for T009 — Helper pins a sentence-transformers that can load the model
USAGE      `uv_args(script)` → `[..., "--with", "sentence-transformers>=6.1", ...]`
CALLS      In `uv_args`, replace the literal `"sentence-transformers>=5"` with `"sentence-transformers>=6.1"`; update the two tests that pin the arg list (verdict_supervisor_test.rs `uv_args_pin_the_verdict_revision`, cli.rs `verdict_serve_args_pin_revisions`) to the new literal. Why: with the Verdict git dependency, `>=5` resolves to 5.7.0, which fails loading google/embeddinggemma-2 with `ModuleNotFoundError: No module named 'sentence_transformers.base.modules.normalize'`; `>=6.1` resolves to 6.1.0 beside it (checked with `uv run --with <all helper deps> python -c "import sentence_transformers"` → 6.1.0). Also run `cargo test --manifest-path rust/Cargo.toml -p worklog-cli verdict_serve_args` and report its executed count.
THE FIVE   (same five as T001)

## Contract for T010 — Warm /stats/topics under 2 s
USAGE      `embed::block_text(&conn, block_id)` — same result as today, ~300× faster on a real DB.
CALLS      In embed.rs `block_text`, the prompt query `FROM block_events be JOIN events e ON e.id = be.event_id` must become `FROM block_events be CROSS JOIN events e ON e.id = be.event_id` (SQLite keeps CROSS JOIN order, so it walks the block's own events via the block_events PK instead of every `source='claude'` event). Measured on a copy of the live DB: 298 blocks, JOIN 5.85 s vs CROSS JOIN 0.02 s; `EXPLAIN QUERY PLAN` goes from `SEARCH e USING INDEX sqlite_autoindex_events_1 (source=?)` first to `SEARCH be USING COVERING INDEX sqlite_autoindex_block_events_1 (block_id=?)` first. Add a test that runs `EXPLAIN QUERY PLAN` on the same SQL text (expose it as a `const PROMPTS_SQL: &str` in embed.rs, pub(crate)) against `db::open_memory()` and asserts the first SEARCH row is on `block_events`. Prove it: with plain JOIN the test fails, with CROSS JOIN it passes. Existing block_text tests must keep passing unchanged.
THE FIVE   (same five as T001)
