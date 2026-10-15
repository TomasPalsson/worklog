# Topic Atlas build plan

The current Topics section has three problems. It names topics after Tempo ticket summaries, which repeats the ticket data. Its clustering collapses into one blob of 84% of blocks. And it shows four unexplained text lists. Topic Atlas replaces it with six visual modules.

Every number is computed in the Rust daemon. The Python helper keeps doing only `/embed`, and the web only draws. Each module shows a "How:" line under its title and a `note` line in its hover cards.

## 0. Shared foundation (Rust, `stats_topics.rs` plus new sibling files)

**F1. Clustering fix (the root cause of the blob).**
- Before clustering, subtract the mean vector and renormalise: `mean = Σv/n; v_i = (v_i−mean)/‖v_i−mean‖`.
- The mean is taken over the `now` window. The same mean is applied to the `history` points used for novelty, so the time comparison stays like-for-like.
- On real data the prototype measured: largest cluster share fell from 84.3% to 21.7%, ticket agreement (AMI) rose from 0.088 to 0.252, and silhouette rose from 0.059 to 0.145.
- Keep the existing average-linkage `cluster()` unchanged and raise `MAX_TOPICS` from 8 to 10. Keep `k = clamp(round(√(n/2)), 2, 10)`.
- Guard: if ‖v−mean‖ < 1e-9, keep the raw vector so there is no NaN.
- Every downstream metric uses the centered vectors.

**F2. c-TF-IDF keywords (new file `topic_terms.rs`, about 120 lines).**
- Tokenise each description: strip Jira keys with `[A-Z][A-Z0-9]+-\d+`, lowercase, split on non-alphanumeric (Unicode-aware so Icelandic letters stay intact), drop tokens shorter than 3 characters, digits, and a short inline stop list of English and Icelandic words.
- Score each term per cluster: `tf(t,c)/len(c) · ln(1 + A / f(t))`, where A is the mean number of words per cluster and f(t) is the term's count across all clusters. This is the BERTopic formula.
- Keep the top 10 terms per cluster.
- Choose representative items with MMR: score = cos(centroid) − 0.5·max cosine to items already picked, with case-insensitive dedupe on description. Pick 5. These feed the naming prompt.

**F3. AI naming with a cache (new file `topic_names.rs`, about 150 lines).**
- New table, bumping `SCHEMA_VERSION` to 22:
  ```sql
  CREATE TABLE IF NOT EXISTS topic_names (
    member_hash TEXT PRIMARY KEY,  -- sha256 of sorted block ids, comma-joined
    members TEXT NOT NULL,         -- the same ids, for Jaccard reuse
    name TEXT NOT NULL, description TEXT NOT NULL,
    model TEXT NOT NULL, created_at TEXT NOT NULL);
  ```
- Lookup, per cluster:
  1. Look for an exact hash match.
  2. Otherwise, scan the 200 newest rows and reuse the name of the best match with Jaccard ≥ 0.6 on members, writing a new row for the new hash. Without this reuse, every new block today would trigger another naming call of 20–30 s.
  3. Otherwise, the cluster is unnamed.
- Unnamed clusters get the keyword fallback immediately: `name = top 3 terms joined with " · "`, `description = None`, `named_by = "keywords"`.
- If any clusters are unnamed, `tokio::spawn` one background job for all of them. Reuse the `daemon_note_block::start_job` shape, including the `MakeInvoker` closure and a `TEST_REPLY` hook.
  - Guard with a static `Mutex<HashSet<String>>` of in-flight hashes so duplicate jobs don't start.
  - The job makes one batched `ModelInvoker::invoke` call through `estimate::build_invoker()` with the default model, `claude-haiku-4-5`.
  - It passes a JSON schema `{topics:[{id,name,description}]}`. Even so, the parser strips `` ```json `` fences defensively.
  - Use the prototype's prompt verbatim, plus one line: "If the items share nothing, name it 'Odds and Ends'."
  - Write the results to `topic_names`.
- The report carries `naming: "ai" | "pending" | "off"`:
  - `"off"` means the invoker failed to build (no model configured). The keyword names stay.
  - The web shows a "naming…" chip while `pending`. A roughly 10-line client component `RefreshSoon` calls `router.refresh()` once after 25 s.
- Writes stay inside the allowed boundary: `block_embeddings` (as today) and `topic_names`. Nothing touches blocks or tickets.

**F4. 2-D map (new file `topic_map.rs`, about 160 lines): exact t-SNE written by hand, no crate.**
- The prototype measured trustworthiness 0.951, kNN agreement 0.80 and 2-D silhouette 0.28. PCA scored 0.825 and looked like mush.
- Parameters:
  - perplexity = `min(30, (n−1)/3)`
  - binary search for σ per point
  - sparse P over the 90 nearest neighbours (3×perplexity), symmetrised
  - initialise from the top 2 principal components (power iteration), scaled to std 1e-4
  - 750 iterations; early exaggeration 12 for the first 250
  - learning rate `max(n/12, 50)`, momentum 0.5 switching to 0.8 at iteration 250
  - repulsion is computed densely each iteration without storing Q
- Memory is O(n·90). It is deterministic, with no RNG.
- Normalise the output to [0,1]² while preserving aspect ratio.
- Add `// ponytail: O(n²) per iter, ~seconds at n=2000; Barnes-Hut if ranges >3k blocks get slow`.
- No layout cache, because the result is deterministic. Add one only if timings say so.
- Skip the map (`points = []`) when n < 10.

**F5. Per-block metrics (inline in `stats_topics.rs` or a new `topic_metrics.rs`, about 120 lines).**
- **novelty(b)** = 1 − max cosine to any *earlier* block, searching `history ∪ now` in `started_at` order.
- **knn_iso(b)** = max cosine to any other block in `now`. A block is an oddball when this is below 0.30; the prototype measured median 0.57 and saw genuine one-offs below 0.3.
- **day.focus** = hours-weighted mean pairwise cosine of that day's blocks. It is `None` when the day has fewer than 2 blocks.
- **day.novelty** = hours-weighted mean of that day's block novelties.
- **day_similarity[i][j]** = cosine of hours-weighted day centroids, computed on centered vectors and rounded to 3 decimals. It covers the most recent 62 days at most.
- Keep `day_switches` exactly as it is today; it moves into the day rows.

**Contract** (`topics_contract.rs` and `web/lib/topics_contract.ts` change together):
```rust
pub struct Topic { id, name, description: Option<String>, named_by: NamedBy /* ai|keywords */,
  keywords: Vec<String> /*≤10*/, seconds, blocks, days_active: i64, first_day: String, last_day: String,
  top_tickets: Vec<Ranked> }
pub struct MapPoint { block_id: i64, x: f32, y: f32, topic_id: usize, day: String, seconds: i64,
  label: String /*description ≤90 chars*/, ticket: Option<String>, novelty: f32, oddball: bool }
pub struct RiverBin { start: String, seconds: Vec<i64> /* len == topics.len() */ }
pub struct DayRow { day, work_seconds, focus: Option<f64>, novelty: f64, switches: i64,
  longest_run_seconds: i64, top_topic: usize, topics_touched: i64 }
pub struct Highlight { block_id, day, label, ticket: Option<String>, topic_id: usize, seconds, score: f64 }
pub struct TopicsReport { status, from, to, naming: Naming /* ai|pending|off */,
  topics, points: Vec<MapPoint>, bin: Bin /* day|week, week when range >90d */, river: Vec<RiverBin>,
  days: Vec<DayRow>, day_similarity: Vec<Vec<f64>> /* same order as days, ≤62 */,
  first_times: Vec<Highlight> /* top 5 novelty */, oddballs: Vec<Highlight> /* knn_iso<0.30, top 5 lowest */,
  embedded_blocks, skipped_blocks }
```

**Removed from the contract:** `Topic.name_ticket`, `switches` (folded into `days`), `drift`, `sprawl`, `similar_days`, `TopicDrift`, `TicketSprawl` and `SimilarDay`. The `before` window loading in `daemon_stats.rs` also goes; `history` stays, now for novelty.

**Degrade rules are unchanged:** `HelperDown` and `TooFewBlocks` return an empty report, and no naming job is started.

## 1. Modules

Topic colours use one rule everywhere: `oklch(var(--project-l) var(--project-c) calc(var(--h)))`, with `--h = 25 + id·(360/K)` set inline. That stays distinct for up to 10 topics and flips correctly in dark mode with no new tokens.

### M1. Topic Galaxy (hero): `TopicGalaxy.tsx` + `galaxy.css`, server-safe
- **What the user learns:** the shape of their work as a star map. Each island is a named theme, and the islands nearest each other are the themes most alike in meaning.
- **Chart:**
  - SVG with `viewBox 0 0 1000 620` on a `--bg-sunk` card with a radial vignette mixed toward `--bg`.
  - Nebula layer: for each topic, circles of r=26 at each member, in a `<g filter="blur(stdDev 16)">` at opacity 0.22.
  - Star layer: dots with `r = 2.5 + 6·√(seconds/max)` and topic fill, plus a 1px `--bg` stroke.
  - Labels: placed at the hours-weighted median x/y of each topic. Name in mono uppercase at `11 + 9·√(share)` px with a `paint-order: stroke` halo, and hours below it in `--fg-muted`. Twenty iterations of vertical nudging resolve collisions.
  - Oddballs get a dashed `--fg-muted` ring at r+5. `first_times` get a small 4-point star.
- **Motion:** dots `.art-fade` in chronological order (`--i` = time rank, delay `min(i·3ms, 1.2s)`), so the galaxy assembles itself in the order the work happened.
- **Interaction:** generated `<style>` rules `.sx-gal:has([data-t="3"]:hover) .sx-gal-dot:not([data-t="3"]){opacity:.12}` per topic, so hovering a label or dot isolates its island with no client JS.
  - Dot tip: title = label; rows = day, hours, ticket, topic, novelty.
  - Label tip: name, description, keywords, hours, blocks.
- **How line:** "Each dot is a work block. The local embedding model turns its text into a vector, the vectors are centered, and t-SNE (perplexity 30) lays them out so similar work sits close together. Topics come from average-linkage clustering. Names: AI (Haiku) from keywords plus examples, or keywords if no model is available. Distance between islands is only roughly meaningful."
- **Degraded state:** `points.length === 0` → "Map needs at least 10 described blocks." When `naming === "pending"`, show a chip "AI is naming topics…" plus `RefreshSoon`.

### M2. Topic River: `TopicRiver.tsx` + `river.css`
- **What the user learns:** when each theme rose and faded. The prototype showed a VÍS sprint from Oct 5–9, Vitinn front-loaded, and LibreChat peaking on 09-30.
- **Chart:**
  - Import `bandPath` (and, if it is not exported yet, the monotone-cubic helper) from `ActivityStream.tsx`. Do not copy it.
  - Centred wiggle baseline (the same centred baseline ActivityStream uses). Layers are ordered by first-active bin so old themes sit low and new ones stack on top.
  - Label each layer inline at its widest bin when the band is at least 14px thick.
  - Month and day axis in `.art-label`. Bands get `.art-grow-y`.
- **Tips:** band → topic name, total hours, peak bin and hours, active bins. Column hover rect → that bin's hours per topic, top 4 rows.
- **How line:** "Hours per topic per day (per week when the range is longer than 90 days). Topics are fitted once over the whole range."
- **Degraded state:** fewer than 2 bins → omitted.

### M3. Topic Heartbeats (the ledger): `TopicHeartbeats.tsx` + `heartbeats.css`
- **Replaces** the `RankedBars` topic list.
- **What the user learns:** what each theme *is*, and whether it is an always-on thread, a sprint or a one-off. The prototype found c6/c2/c4 active on 12–13 days, and c5 spanning 5 days, all inside Oct 5–9.
- **Row layout** (CSS grid, stacking on mobile):
  - swatch and mono name
  - description (sans, muted) and up to 5 keyword chips
  - raster: one tick per bin from `river`, height ∝ hours, topic colour
  - hours bar using `.sr-fill` styling
  - badge: **Always on** (`days_active ≥ 0.5 × work days in range`), **Sprint** (span ≤ 7 days and `days_active ≥ 3`) or **One-off** (`days_active == 1`); otherwise none
  - top ticket keys in small mono, as context only, with no hours
- **Data:** derived on the web from `topics` and `river`. Put the badge rule in a pure `rhythmOf(topic, workDays)` with its own test.
- **How line:** "Ticks show hours per day in this topic. Always on = active on at least half your work days. Sprint = at least 3 days, all within one week."

### M4. Day Mirror (self-similarity): `DayMirror.tsx` + `mirror.css`
- **Replaces** "Similar days".
- **What the user learns:** which days were twins and which were opposites, and the "chapters" of the period, visible as bright blocks along the diagonal. The prototype measured 10-05~10-07 at 0.535 and 09-23 vs 10-05 at −0.522.
- **Chart:**
  - Upper-triangle staircase of D×D square cells, with day labels running down the diagonal.
  - Diverging fill: `color-mix(in oklch, var(--amber) p%, var(--bg-sunk))` for positive values and `--slate` for negative, where p = |sim|/max|sim|·100. Redeclare the mixes under both dark selectors.
  - Each diagonal cell is a topic-colour chip of that day's `top_topic`.
  - Cells get `.art-fade` with `--i = i+j`, so the matrix sweeps in along the diagonal.
- **Tip:** "Oct 5 ↔ Oct 7", rows: similarity, each day's top topic and hours. The note says "+1 means the same kind of work, −1 means opposite."
- **How line:** "Cosine similarity of each day's hours-weighted average vector, centered so ordinary days score near 0."
- **Degraded state:** D < 3 → omitted.

### M5. Day Compass (focus × novelty quadrant): `DayCompass.tsx` + `compass.css`
- **Replaces** "Switches per day".
- **What the user learns:** which days were deep or scattered, and which were familiar ground or new territory. The prototype measured focus from −0.017 to 0.324 and novelty from 0.48 to 0.73.
- **Chart:**
  - SVG `0 0 480 320`. x = focus, y = novelty.
  - Crosshairs at the **medians**, not fixed thresholds, so the quadrants are honest about being relative.
  - Quadrant captions in `.art-label`: "Deep · new ground", "Deep · familiar", "Scattered · exploring", "Scattered · routine".
  - Dots: r ∝ √hours, coloured by top topic. The extreme day in each quadrant gets a leader-line label with its short weekday and date.
  - Days with `focus: None` are listed under the chart as "single-block days: …" and are not plotted.
- **Tip:** day, hours, focus, novelty, switches, longest run (`formatDuration`), topics touched.
- **How line:** "Focus = how alike that day's blocks are (hours-weighted mean pairwise cosine). Novelty = how unlike anything you did before (1 − nearest earlier block). Lines are your medians."

### M6. First Times & Oddballs: `TopicMoments.tsx` + `moments.css`
- **What the user learns:** the most genuinely new things they started, and the one-offs that match nothing else. The prototype found "Implement typed per-client state rows" at 0.231, and first-time spikes such as 0.85 for "Merge billing view…".
- **Chart:**
  - Two short card rows in `.stats-two`.
  - Each card: a large mono score, the label, day and hours, a topic chip, and a mini 1-D strip showing where this block sits in the range timeline.
  - The oddballs card also shows "nearest anything: 0.23", next to the median for comparison.
- **How line:** "First time = lowest similarity to any earlier block, including the 60 days before the range. Oddball = its closest block in this range is under 0.30 cosine; your median block's closest is {median}."
  - This needs `oddball_median: f64` added to the report (one extra field).
- **Degraded state:** an empty list renders one `.sr-empty` line, for example "Nothing here stands apart; your work is well clustered."

**Page order inside the Topics `Section`:** M1 → M3 → M2 → M4 and M5 side by side in `.stats-two` → M6. Rename the section heading to "Topic atlas".

## 2. Drop list (current section)
- `RankedBars` "Topics": replaced by M3.
- "Switches per day" list: folded into the M5 tips.
- "Drift": weak and unexplained. Remove it along with the `before`-window code path.
- "Catch-all tickets" (sprawl): ticket-centric, which the user asked to avoid, and unexplained. Remove it.
- "Similar days" list: replaced by M4.
- Topic names taken from ticket summaries (`topic_name`, `name_ticket`).
- The local `Card` helper in `TopicsSection.tsx`, if nothing else uses it.

Skipped on purpose:
- **Weekly drift and comet trajectory.** The prototype found drift noisy (0.60–1.29). Add it once there are months of data.
- **Ticket-alignment Sankey.** Too ticket-centric for what was asked.
- **Chord transitions.** Not validated on this data.
- **Foote novelty curve.** M4 shows the chapters visually without a fitted kernel.

## 3. Task breakdown

**Slice A, Rust core** (blocks B; can run alongside C, which mocks against the new TS contract):
- **A1.** `topics_contract.rs`: new structs, `MAX_TOPICS=10`, constants `ODDBALL_MAX_COS=0.30`, `FIRST_TIMES_TOP=5`, `MIRROR_MAX_DAYS=62`, `RIVER_WEEK_AFTER_DAYS=90`. Mirror the same in `web/lib/topics_contract.ts`.
- **A2.** Centering plus metrics in `stats_topics.rs`, splitting into `topic_metrics.rs` to stay under the size hook.
  - Rewrite `stats_topics_test.rs`: drop the drift, sprawl and similar-days tests.
  - Add tests: centering separates an anisotropic fixture (two groups sharing a large common component that raw clustering merges) ≥ 0.9 by group; novelty is 1 for the first-ever block; oddball threshold; focus is `None` for a single-block day; day_similarity is symmetric with a diagonal of 1; river columns sum to topic seconds.
- **A3.** `topic_terms.rs`: c-TF-IDF plus MMR. Tests: Jira keys stripped, Icelandic tokens kept, MMR never repeats a description.
- **A4.** `topic_map.rs`: t-SNE. Tests: deterministic (two runs are bit-identical); two well-separated groups of 15 end with mean intra-group distance < 0.3 × inter-group distance; output stays within [0,1]; n<10 → empty.

**Slice B, Rust naming and wiring** (after A1):
- `topic_names.rs`, the schema table and migration in `sql/schema.sql` plus `db.rs` (version 22), the background job in `daemon_stats.rs`, and removal of the `before` loading.
- Tests:
  - an exact-hash hit makes no invoker call
  - Jaccard ≥ 0.6 reuses the old name and inserts a new row; 0.5 does not
  - a fenced JSON reply parses
  - an invoker build error gives `naming: off` with keyword names
  - a route-level test via `TEST_REPLY`: first GET is `pending`, and after the job completes the second GET is `ai`
  - nothing is written to `blocks`

**Slice C, web** (parallel per module once A1's TS mirror exists; separate files mean no worktree isolation is needed):
- C1 M1 Galaxy
- C2 M2 River (export helpers from `ActivityStream.tsx`)
- C3 M3 Heartbeats
- C4 M4 Mirror
- C5 M5 Compass
- C6 M6 Moments
- C7 rewrite `TopicsSection.tsx`, add `RefreshSoon.tsx`, update `TopicsSection.test.tsx`

Each Cn has a pure layout function, a pure `*Tip` builder with exact-value tests, and one render test asserting no "NaN", "undefined" or "null".

## 4. Acceptance checks
1. All gates are green:
   - `cargo test`, `cargo clippy … -D warnings`, `cargo fmt --check`
   - `cd web && bun test && bun run typecheck && bun run build`, with the dev server stopped first
2. On the real DB copy (`/Users/tomas/.claude/jobs/491be4f8/tmp/proto/wl.db`, 198 blocks): the largest topic holds ≤ 25% of blocks, and k=10.
3. On the same data, `/stats/topics` returns `ready` in < 3 s on a warm cache, excluding naming.
4. t-SNE at a synthetic n=2000 takes < 10 s in a release build, measured by an `#[ignore]` bench test.
5. A second GET with unchanged data makes zero model calls (checked with a counter on the test invoker). Adding one block keeps every topic name (Jaccard reuse).
6. With `VERDICT_ENABLED=off` or the helper down, the "need the local helper" message shows and `topic_names` gets no rows. With no model configured, keyword names show and the "naming" chip does not appear.
7. Every module shows a visible "How:" line, and every mark has `tipProps`, `tabIndex=0` and an `aria-label`.
8. Light and dark modes both render with no hard-coded colours (`grep -E '#[0-9a-f]{3,6}'` on the new CSS finds nothing). At 375 px there is no horizontal scroll.
9. A grep for drift, sprawl, similar_days and name_ticket finds no matches in `web/` or `rust/`.

**Open question for the orchestrator:** should "Topic atlas" replace the Topics section in place (the assumption above), or become its own section after Tickets? Moving it is a one-line change in `app/stats/page.tsx`.