# Design — Topic atlas

Full computations and chart forms: plan.md (§0 F1–F5, §1 M1–M6). Measured numbers: prototype.md. Page tokens: design-system.md.

## 1. Contract files

- Rust: `rust/crates/worklog-core/src/topics_contract.rs` — orchestrator-owned. Report types: `Topic`, `MapPoint`, `RiverBin`, `DayRow`, `Highlight`, `TopicsReport`, enums `NamedBy`, `Naming`, `Bin`; constants `MAX_TOPICS=10`, `MAP_MIN_POINTS=10`, `ODDBALL_MAX_COS=0.30`, `HIGHLIGHTS_TOP=5`, `MIRROR_MAX_DAYS=62`, `RIVER_WEEK_AFTER_DAYS=90`, `TOPIC_KEYWORDS=10`, `NAMING_EXAMPLES=5`, `NAME_REUSE_JACCARD=0.6`.
- Web mirror: `web/lib/topics_contract.ts` (snake_case, same fields).
- Module stubs registered in lib.rs by the orchestrator: `topic_metrics`, `topic_terms`, `topic_map`, `topic_names`. Tasks never edit lib.rs.

| canonical | identifier | banned synonyms |
|---|---|---|
| topic | `Topic` / topic_id (usize, 0-based by seconds desc) | cluster (in JSON/UI), theme_id |
| centered vector | `Vec<f32>` after mean-subtract + renormalise | whitened, debiased |
| map point | `MapPoint` | node, star (in code) |

## 2. Trust boundaries

| boundary | shape | parse | failure |
|---|---|---|---|
| model invoker reply (naming) | text, maybe ```json fenced | strip fences → serde `{topics:[{id,name,description}]}`; name ≤ 40 chars, description ≤ 200 chars, else keyword fallback for that id | per topic |
| `topic_names` row | TEXT | serde of members list | bad row → ignored |

## 3. Errors

No new error type. Naming failures never fail the route: keyword names + `Naming::Pending` or `Off`. DB errors propagate as today.

## 5. Shared resources

| resource | constructed by | passed how | received by |
|---|---|---|---|
| centered vectors | `topic_metrics::center(&[Vec<f32>]) -> Vec<Vec<f32>>` (T002) | value | stats_topics, topic_map (T005 passes them) |
| model invoker | `estimate::build_invoker()` | closure `MakeInvoker` like daemon_note_block::start_job | topic_names (T005) |
| topic colour | CSS `oklch(var(--project-l) var(--project-c) calc(var(--h)))`, `--h = 25 + id*(360/K)` set inline as `style={{"--h": ...}}` | prop `topicCount` | every web module |

## 6. Deliberately duplicated

- Each web module owns its pure layout fn and its `*Tip` builder; do not create a shared atlas utils file. Exception: T008 exports `bandPath` / monotone helper from ActivityStream.tsx and imports them (no copy).

## 7. Decisions

- Centering before clustering (prototype: largest 84.3% → 21.7%), rejected HDBSCAN (47–69% noise at this n). Makes hard: stats_topics.rs.
- Exact t-SNE in Rust (trust 0.951 vs PCA 0.825), rejected a Python /layout endpoint (adds heavy deps to the helper). Makes hard: topic_map.rs.
- Names cached by sorted-member sha256 with Jaccard ≥ 0.6 reuse, named in a background job; rejected naming inline (20–30 s per call). Makes hard: topic_names.rs, daemon_stats.rs.

## Contract for T002 — Centered clustering + per-block/day metrics
USAGE      `let c = topic_metrics::center(&vecs); let labels = stats_topics::cluster(&c, k);` then `topic_metrics::{novelty, nearest_cos, day_rows, day_similarity, river}` feed `stats_topics::build(&TopicsInput) -> TopicsReport` (keeping the `TopicsInput`/`BlockPoint` shapes; drop `before`; add nothing to them except what's listed).
CONTRACT   topics_contract.rs — import from it.
CALLS      `pub fn center(v: &[Vec<f32>]) -> Vec<Vec<f32>>` (mean over all rows; (v−mean)/‖v−mean‖; if norm < 1e-9 keep v). `build` must: center `now`; cluster with existing average-linkage, k = clamp(round(√(n/2)), MIN_TOPICS, MAX_TOPICS); novelty(b) = 1 − max cos(b, any block with started_at < b's, from history ∪ now), all centered with the `now` mean (first-ever block → 1.0); nearest_cos(b) = max cos to any other block in now; oddball = nearest_cos < ODDBALL_MAX_COS; `oddball_median` = median nearest_cos; DayRow per day (focus = hours-weighted mean pairwise cos, None if < 2 blocks; novelty = hours-weighted mean; switches/longest_run as today's day_switches; top_topic by seconds; topics_touched); `day_similarity` = cos of hours-weighted day centroids (centered), last ≤ MIRROR_MAX_DAYS days, symmetric, diag 1, rounded to 3 dp; `river` = every day (or ISO week Monday when range > RIVER_WEEK_AFTER_DAYS days) from `from` to `to`, seconds per topic; `first_times` top HIGHLIGHTS_TOP by novelty; `oddballs` oddball blocks lowest nearest_cos first, top HIGHLIGHTS_TOP; Topic fields `days_active/first_day/last_day/top_tickets`. Leave `name` = "" , `description` None, `named_by` Keywords, `keywords` empty, `points` empty, `naming` Off — T005 fills those. Remove drift / sprawl / similar_days / name_ticket code and their tests; keep the tie-break determinism tests that still apply. Split code so no file exceeds the repo's size hook.
TESTS      anisotropic fixture (two groups + big shared component) → split ≥ 0.9 by group; first block novelty == 1.0; oddball threshold; focus None for single-block day; day_similarity symmetric, diag 1; river columns sum to topic seconds; week binning above 90 days.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T003 — c-TF-IDF keywords + MMR examples
USAGE      `let terms: Vec<Vec<String>> = topic_terms::keywords(&docs_per_topic, TOPIC_KEYWORDS); let ex: Vec<usize> = topic_terms::representatives(&centered_vecs_of_topic, &descriptions_of_topic, NAMING_EXAMPLES);`
CALLS      `pub fn tokenize(text: &str) -> Vec<String>`: strip `\b[A-Z][A-Z0-9]+-\d+\b`, lowercase, split on non-alphanumeric (Unicode: keep ð þ æ ö á é í ó ú ý), drop tokens < 3 chars and pure digits, drop stopwords (inline const: the ask.rs English+Icelandic list plus: work, worked, working, task, tasks, update, updated, fix, fixed, add, added, review, check, make, made, using, use, new, into, with, from, this, that, over, also, och — keep it short). `pub fn keywords(docs_per_topic: &[Vec<String>], top: usize) -> Vec<Vec<String>>`: c-TF-IDF `tf(t,c)/len(c) · ln(1 + A / f(t))`, A = mean tokens per topic, f(t) = count of t over all topics; ties by term asc. `pub fn representatives(vecs: &[Vec<f32>], descriptions: &[String], n: usize) -> Vec<usize>`: MMR — score = cos(v, centroid) − 0.5·max cos(v, picked); skip a description equal (case-insensitive, trimmed) to a picked one; deterministic ties by lower index.
TESTS      Jira keys stripped; Icelandic tokens kept intact; stopwords gone; a term unique to one topic outranks a term common to all; MMR never returns two equal descriptions; returns ≤ n indices.
THE FIVE   (same five as T002)

## Contract for T004 — Exact t-SNE map
USAGE      `let xy: Vec<[f32; 2]> = topic_map::layout(&centered_vecs);` → empty when n < MAP_MIN_POINTS.
CALLS      `pub fn layout(v: &[Vec<f32>]) -> Vec<[f32; 2]>`: perplexity = min(30, (n−1)/3); per-point σ by binary search on cosine-distance (1 − cos) affinities; sparse P over the 3·perplexity nearest neighbours, symmetrised P = (P + Pᵀ)/2n; init = top-2 principal components by power iteration (fixed start vector, no RNG), scaled to std 1e-4; 750 iterations, early exaggeration 12 for the first 250; learning rate max(n/12, 50); momentum 0.5 → 0.8 at iteration 250; gains as in van der Maaten (+0.2 / ×0.8, min 0.01); repulsion dense each iteration without storing Q. Normalise output to [0,1]² preserving aspect ratio (centre the shorter axis). Add `// ponytail: O(n²) per iteration, ~seconds at n=2000; Barnes-Hut if ranges >3k blocks get slow`.
TESTS      deterministic (two runs bit-identical); two groups of 15 well-separated vectors → mean intra-group distance < 0.3 × inter-group distance; all coords in [0,1]; n < 10 → empty; `#[ignore]` bench n=2000 printing elapsed.
THE FIVE   (same five as T002)

## Contract for T005 — AI naming cache + route wiring
USAGE      daemon_stats `topics_finish`: after `stats_topics::build`, call `topic_terms` for keywords + representatives, `topic_map::layout` for points (map MapPoint from the in-range blocks: label = description ≤ 90 chars, novelty/oddball from the report), then `topic_names::resolve(conn, &mut report, &topic_inputs) -> NamesOutcome` which fills name/description/named_by and returns topics still needing AI names; if any and an invoker can be built, `topic_names::start_job(...)` in the background and set `naming = Pending`; if the invoker can't be built → `Off`; all named → `Ai`.
CALLS      schema.sql: `CREATE TABLE IF NOT EXISTS topic_names (member_hash TEXT PRIMARY KEY, members TEXT NOT NULL, name TEXT NOT NULL, description TEXT NOT NULL, model TEXT NOT NULL, created_at TEXT NOT NULL);` (no version bump — CREATE IF NOT EXISTS runs every open, as block_embeddings). member_hash = sha256 hex (updater::crypto sha256_hex) of the sorted block ids joined by ","; lookup: exact hash; else newest 200 rows, best Jaccard ≥ NAME_REUSE_JACCARD → reuse name and insert a row for the new hash; else keyword fallback `name = top 3 keywords joined " · "`, description None. Background job mirrors `daemon_note_block::start_job` (MakeInvoker closure, `TEST_REPLY` hook, static `Mutex<HashSet<String>>` of in-flight hashes); ONE batched call through `estimate::build_invoker()` (default Haiku) with the prompt in plan.md §0 F3 / prototype.md §4 verbatim plus "If the items share nothing, name it 'Odds and Ends'."; parse with fence stripping; store rows. Remove the `before` window loading. Keep the three-phase connection pattern (no DB lock during /embed or the model call).
TESTS      exact-hash hit → 0 invoker calls; Jaccard 0.6 reuses + inserts, 0.5 does not; fenced JSON parses; invoker build error → `Off` + keyword names; route: first GET `pending`, after job completes GET `ai`; table snapshot: only block_embeddings and topic_names change; helper down → helper_down, no topic_names rows.
THE FIVE   (same five as T002)

## Contract for T013 — Atlas art set
USAGE      `import { RhythmIcon, AtlasEmpty } from "./AtlasArt"; <RhythmIcon kind="sprint" />; <AtlasEmpty kind="helper_down" />`
CALLS      Implement exactly the SVG assets delivered by the design:graphics skill at `.specs/023-topic-atlas/art/` (GRAPHICS.md lists ids, alt text, sizes) as React components using `currentColor` / page tokens only. Exports: `RhythmIcon({kind: "always" | "sprint" | "one_off"})` (16px, aria-hidden, paired with text by callers), `FirstTimeMark`, `OddballMark` (14px), `AtlasEmpty({kind: "helper_down" | "too_few" | "no_map"})` (spot illustration ~120px with its alt text).
TESTS      each export renders an <svg> with no hard-coded hex colours; AtlasEmpty has role="img" + the GRAPHICS.md alt text.
THE FIVE   (same five as T002)

## Contract for T006 — Topic galaxy
USAGE      `<TopicGalaxy report={report} />` (server-safe, no client JS).
CALLS      plan.md §1 M1 exactly: SVG viewBox 0 0 1000 620 on a `--bg-sunk` card with radial vignette; nebula layer (r=26 circles per member, blur stdDev 16, opacity .22); stars r = 2.5 + 6·√(seconds/max), topic fill, 1px `--bg` stroke; labels at hours-weighted median x/y, mono uppercase at 11 + 9·√share px with paint-order stroke halo + hours under in `--fg-muted`, 20 iterations vertical nudge for collisions; oddballs dashed ring r+5 with `OddballMark`; first_times with `FirstTimeMark`; `.art-fade` with `--i` = chronological rank, delay min(i·3ms, 1.2s); per-topic generated `:has()` hover isolation rules; tips via the page's tip system (dot: label, day, hours, ticket, topic, novelty; label: name, description, keywords, hours, blocks); "How:" line text from plan.md M1; empty → `<AtlasEmpty kind="no_map"/>` + "Map needs at least 10 described blocks."; `naming === "pending"` chip "AI is naming topics…". Pure `galaxyLayout(report)` + `galaxyTip*` exported and tested.
TESTS      layout labels don't overlap after nudge for a crowded fixture; every dot has tabIndex 0 + aria-label; no "NaN"/"undefined"/"null" in markup; pending chip shows only when pending.
THE FIVE   (same five as T002)

## Contract for T007 — Topic heartbeats
USAGE      `<TopicHeartbeats report={report} />`
CALLS      plan.md §1 M3: one row per topic — swatch + mono name, description (muted) + ≤ 5 keyword chips, raster of one tick per river bin (height ∝ hours, topic colour), hours bar `.sr-fill` styling, rhythm badge with `RhythmIcon` + text, top ticket keys in small mono (no hours). Pure `rhythmOf(topic, workDays): "always" | "sprint" | "one_off" | null` — always: days_active ≥ 0.5·workDays; sprint: (last_day − first_day) ≤ 6 days and days_active ≥ 3; one_off: days_active == 1. Grid stacks on mobile. "How:" line from plan.md M3. Keyword-named topics show the keywords as the name in muted italic.
TESTS      rhythmOf boundaries (each rule ±1); row count == topics; tip values exact.
THE FIVE   (same five as T002)

## Contract for T008 — Topic river
USAGE      `<TopicRiver report={report} />`
CALLS      plan.md §1 M2: export `bandPath` and the monotone-cubic helper from ActivityStream.tsx (no behaviour change there) and import them; centred wiggle baseline; layers ordered by first active bin; inline labels at widest bin when band ≥ 14px; axis labels `.art-label`; `.art-grow-y`; band tip (name, total hours, peak bin + hours, active bins) and column hover tip (bin's top 4 topics); "How:" line; omitted when < 2 bins.
TESTS      stacked heights per bin == bin seconds scaled; ActivityStream existing tests still pass; no NaN with an all-zero bin.
THE FIVE   (same five as T002)

## Contract for T009 — Day mirror
USAGE      `<DayMirror report={report} />`
CALLS      plan.md §1 M4: upper-triangle staircase of D×D cells over `days` (last D = day_similarity.length), day labels down the diagonal; diverging fill — positive `color-mix(in oklch, var(--amber) p%, var(--bg-sunk))`, negative `--slate`, p = |sim|/max|sim|·100, mixes redeclared for both dark selectors; diagonal cell = top_topic colour chip; `.art-fade` with `--i = i+j`; tip "Oct 5 ↔ Oct 7" + similarity + each day's top topic and hours, note "+1 = same kind of work, −1 = opposite"; "How:" line; omitted when D < 3.
TESTS      cell count == D(D+1)/2; sign → colour family; tip text exact.
THE FIVE   (same five as T002)

## Contract for T010 — Day compass
USAGE      `<DayCompass report={report} />`
CALLS      plan.md §1 M5: SVG 0 0 480 320, x = focus, y = novelty, crosshairs at medians, quadrant captions ("Deep · new ground", "Deep · familiar", "Scattered · exploring", "Scattered · routine"), dots r ∝ √hours coloured by top_topic, leader-line label on the extreme day per quadrant, focus-None days listed under the chart; tip (day, hours, focus, novelty, switches, longest run via formatDuration, topics touched); "How:" line.
TESTS      median lines at the computed medians; quadrant assignment; None-focus days not plotted but listed.
THE FIVE   (same five as T002)

## Contract for T011 — First times & oddballs
USAGE      `<TopicMoments report={report} />`
CALLS      plan.md §1 M6: two card rows in `.stats-two`; card = large mono score, label, day + hours, topic chip, mini 1-D strip of where the block sits in the range; oddball card shows "nearest anything: 0.23" vs the median (`oddball_median`); `FirstTimeMark` / `OddballMark`; "How:" line with the median filled in; empty list → one `.sr-empty` line ("Nothing here stands apart; your work is well clustered.").
TESTS      ordering preserved; empty states; strip position fraction exact.
THE FIVE   (same five as T002)

## Contract for T012 — Topic atlas section assembly
USAGE      page.tsx keeps streaming `getTopics(from,to).catch(() => null)` through Suspense; Section title becomes "Topic atlas".
CALLS      TopicsSection renders: null / helper_down → `<AtlasEmpty kind="helper_down"/>` + the existing helper message; too_few_blocks → `<AtlasEmpty kind="too_few"/>` + existing message; ready → Galaxy → Heartbeats → River → (Mirror | Compass in `.stats-two`) → Moments. `RefreshSoon` = tiny client component calling `router.refresh()` once after 25 s, rendered only when `naming === "pending"`. Remove the old Card helper and every drift/sprawl/similar-days/name_ticket reference in web/.
TESTS      each status renders its state; ready renders all six module roots; pending renders RefreshSoon; grep-style test that "drift"/"sprawl"/"similar_days" don't appear in the output.
THE FIVE   (same five as T002)

## Contract for T014 — Route drops the before window so the crate compiles
USAGE      `cargo test -p worklog-core daemon_stats` compiles and passes again after T002 removed `TopicsInput.before` and `TopicsReport.drift/sprawl/similar_days/switches`.
CALLS      In daemon_stats.rs delete the previous-range (`before_from`, `before` pushes) loading and anything feeding `TopicsInput.before`; history (60 days before `from`) stays. In daemon_stats_test.rs remove only the assertions on removed fields (`r.drift`, sprawl, similar_days, switches) — do not weaken any other assertion; where a test only existed to prove drift/before windows, replace it with one asserting the same window arithmetic still feeds `history` correctly. Nothing else changes (naming, map, keywords are T005).
THE FIVE   (same five as T002)

## Contract for T015 — Galaxy: labels stay inside the map, keyboard isolation, fewer tab stops
USAGE      `<TopicGalaxy report={report} />` unchanged API.
CALLS      Review findings (Chromium-measured): (a) topic labels are clamped in y but never in x — "Billing export · invoice · customer" rendered x0 −93..x1 231 (card has overflow:hidden). Fix in `galaxyLayout`: estimate width (existing name.length×font×0.62), shrink font down to a 11px floor, then truncate the name with "…" (full name stays in the tip and aria-label), then clamp x so the whole box sits within [8, 992]; run the vertical nudge AFTER the clamp and re-clamp, iterate to convergence (≤ 20 rounds). (b) add `:focus-visible` (and `:focus-within`) alongside `:hover` in the generated per-topic isolation rules. (c) tab stops: dots get `tabIndex={-1}` (still `data-stip` for pointer); topic labels stay `tabIndex=0` and their tip gains "Blocks: N · first … last". (d) use topic `id` (not array position) for hue: `25 + id*(360/K)` unrounded, same formula as the other modules.
TESTS      a label whose island sits at x=1 with a long keyword name ends fully within [8, 992] (assert box edges); after clamp no two labels overlap (6-label crowd at the right edge); generated CSS contains `:focus-visible`; dots have tabindex −1, labels 0; hue for id 3 of K=10 equals 133.
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T016 — River bands hoverable; compass labels inside the plot
USAGE      `<TopicRiver report={report}/>`, `<DayCompass report={report}/>` unchanged.
CALLS      River (measured: 0 of 10 bands hit by mouse; the full-height transparent `rect.sx-river-col` column rects are painted after the bands and catch every pointer): draw the column rects BEFORE the bands (or give them `pointer-events: none` and put the column tip on the axis strip below the chart) so a pointer over a band gets the band tip and a pointer over empty space gets the column tip. Keep `tabIndex` behaviour. Compass (measured: "Fri Sep 25" x 439..485 cut at 480; "Sun Sep 27" x −9; "Sat Sep 19" overlaps the "Scattered · exploring" caption at y 13): place each leader label with an estimated width (text.length×6.2 at 10px mono), flip its anchor to the inner side when it would cross 8..472, clamp y into [24, 296], and if its box intersects a quadrant caption box, move it vertically by the overlap + 4px.
TESTS      river: in the rendered SVG the band paths come after the column rects in document order OR the column rects carry pointer-events none (assert whichever you implement) and band tips still exist; compass: for a fixture with extreme days at the plot edges, every label box is within [8, 472]×[24, 296] and intersects no caption box.
THE FIVE   (same five as T015)

## Contract for T017 — Mirror labels fit, one tab stop per day
USAGE      `<DayMirror report={report}/>` unchanged.
CALLS      Measured: at D=30 in a half-width card the cell is 15px and every diagonal label (23px) overflows; D=62 → 1953 tab stops and ~1.2 MB HTML. Fix: (a) labels — compute cell px from the viewBox (assume the card is the `.stats-two` half width ~520px when unknown); show a label only every k-th day where k = ceil(labelPx / cellPx), always label the first and last day, and render labels OUTSIDE the matrix along the left edge (row headers) instead of on the diagonal when cell < 22px; (b) a11y/payload — similarity cells get no `tabIndex` and no per-cell `aria-label`; keep `data-stip` on cells for pointer; add one focusable diagonal chip per day (`tabIndex=0`, `aria-label`, tip: "Most like: <day> (+0.53) · Least like: <day> (−0.52) · Top topic: <name>"); (c) hue `25 + id*(360/K)`.
TESTS      at D=30 the number of rendered day labels ≤ ceil(30 / k) + 1 and includes first and last; tab stops == D; cells have no tabindex; diagonal tip names the correct most/least similar day for a fixture.
THE FIVE   (same five as T015)

## Contract for T018 — Naming chip keeps checking until names land
USAGE      `<RefreshSoon />` rendered by TopicsSection while `naming === "pending"`.
CALLS      Measured: a real naming job takes 30–120 s, so one refresh at 25 s leaves the chip up forever. Change to: refresh after 20 s, then every 20 s while still mounted, at most 6 refreshes (≈ 2 min), stop on unmount. The component stays a tiny client component with no props beyond optional `intervalMs` / `maxTries` for tests.
TESTS      with fake timers: refresh called once at 20 s, 6 times by 120 s, never a 7th; no call after unmount.
THE FIVE   (same five as T015)

## Contract for T019 — Naming that finishes: batched calls, longer timeout, failure back-off, one clustering
USAGE      GET /stats/topics → `naming: pending` → within ~2 min a later GET returns `ai` on the live-data copy (10 topics).
CALLS      Live failure: "topic naming failed: claude -p timed out after 60s" for a 10-topic batch, and every GET re-launched it. Fix in topic_names.rs / daemon_stats.rs: (a) name in batches of ≤ 4 topics per model call, run sequentially in the one background job, store each batch's rows as soon as it returns (a later batch failing keeps earlier names); (b) give the naming invocation a 180 s timeout — find where the invoker's 60 s comes from (estimate.rs / claude_subprocess.rs) and add a timeout parameter or a dedicated constructor used only by naming, without changing other callers' timeouts; (c) failure back-off: a static map member_hash → last failure Instant; skip starting a job for a hash that failed < 10 minutes ago and report `naming: off` for that load so the chip doesn't show; (d) remove the duplicated clustering in daemon_stats (`atlas` regroups blocks itself): make `stats_topics::build` return (or expose via a companion fn) the per-block topic id, novelty and oddball for `now` blocks in input order, and use that for map points, keywords documents and naming membership.
TESTS      batches: 10 topics with a test invoker → 3 calls (4+4+2), all names stored; second batch erroring keeps first batch's 4 names; back-off: after a failure the next GET does not invoke and reports off, and after the window (inject a clock or make the window a parameter in tests) it invokes again; membership: map point topic ids equal build's topic ids for the same input (no second clustering code path — grep-free assertion via the returned assignments).
THE FIVE   (same five as T015)

## Contract for T020 — Naming calls get a 180 s timeout
USAGE      the naming job's model call may run up to 180 s; every other `claude -p` caller keeps today's 60 s default.
CALLS      Live: batch 3 of 3 ("topic naming failed: claude -p timed out after 60s"); 60 s = `DEFAULT_TIMEOUT_SECS` in claude_subprocess.rs; `timeout_secs` is private and only set by `with_thinking`. Add `pub fn with_timeout(secs: u64) -> Self` (thinking_tokens None, timeout_secs Some(secs)) in claude_subprocess.rs; add `pub fn build_naming_invoker()` in estimate.rs mirroring `build_invoker` (same LiteLLM/claude selection) but using `ClaudeSubprocess::with_timeout(180)` for the subprocess path (LiteLLM path: use a 180 s HTTP timeout if it has one, else unchanged); point daemon_stats.rs `real_invoker` (the naming MakeInvoker) at `build_naming_invoker`. No other caller changes.
TESTS      `with_timeout(180)` sets 180 and no thinking; `default()` still 60; naming invoker path uses the new constructor (unit-test the constructor; route tests keep their test invoker).
THE FIVE   (same five as T015)

## Contract for T021 — River starts at the first data day; labels never overlap
USAGE      `<TopicRiver report={report}/>`
CALLS      Seen in Chrome on real data (30d range, data starts Sep 23): the left ~40% of the chart is a flat empty line, and inline labels collide ("MULTI-CUSTOME…" under "BILLING · BINDING · BLOCK", "APRÓ SKILLS FRAMEWORK" touching "SUPPORT AGENT INFRASTRUCTURE"). Fix in `buildRiver`: trim leading and trailing bins whose total is 0 before laying out (axis labels show the trimmed first/last bin); labels: estimate box (text.length × 6.6 at 11px mono, height 12), place each candidate at its widest bin, process largest topics first, skip a label whose box intersects an already placed label (+4px pad) or does not fit inside its band thickness (≥ 14px rule stays) or the viewBox; topics without an inline label are listed in a compact legend row under the chart (swatch + name), so every topic is still named.
TESTS      leading/trailing zero bins trimmed (first axis label = first non-zero bin); placed label boxes pairwise disjoint for a crowded fixture; every topic appears either inline or in the legend.
THE FIVE   (same five as T015)

## Contract for T022 — Heartbeats columns line up across rows
USAGE      `<TopicHeartbeats report={report}/>`
CALLS      Seen in Chrome: the raster/bar column starts at a different x per row (633, 638, 655 px) because the grid columns size to content. Make the row a fixed template: `grid-template-columns: minmax(150px, 220px) minmax(0, 1fr) 360px` (name+badge | description+chips | raster+bar+tickets) at ≥ 800px, stacking below as today; raster and bar take the full third column width; long names wrap within column 1.
TESTS      every row element uses the same grid class/template (assert the CSS rule string and that rows have no inline width overrides); render with a long and a short description still yields the same column classes.
THE FIVE   (same five as T015)

## Contract for T023 — Novelty ignores the warm-up at the start of history
USAGE      `first_times` and the compass novelty axis only judge blocks that had something earlier to be compared with.
CALLS      Seen on real data: the top 4 "first times" are all from Sep 23, the first day with any embedded block, so their novelty (1.00, 0.99, 0.91, 0.88) only says "nothing came before". Add `pub const NOVELTY_WARMUP_BLOCKS: usize = 20;` LOCALLY in topic_metrics.rs (do not edit the contract file); a block's novelty is computed only when ≥ NOVELTY_WARMUP_BLOCKS blocks precede it (history ∪ now, by started_at); otherwise it is excluded from `first_times`, from DayRow.novelty averaging (a day whose blocks are all in warm-up gets novelty = the median of the other days' novelty so it plots neutrally — document in a comment) and MapPoint.novelty is 0.0 for them. TopicMoments How line gains: "Your first 20 blocks are skipped — they have nothing earlier to compare with."
TESTS      rust: with 25 blocks, the first 20 never appear in first_times; block 21 can; day of only warm-up blocks gets the median; web: How line contains the warm-up sentence.
THE FIVE   (same five as T015)

## Contract for T024 — Compass card sizes to its content
USAGE      Compass sits beside the Mirror in `.stats-two`.
CALLS      Seen in Chrome: the compass card stretches to the mirror's height leaving ~300px empty. In compass.css set the card (`.sx-cmp` root or its card wrapper) `align-self: start`. CSS only.
TESTS      DayCompass tests still pass (no markup change).
THE FIVE   (same five as T015)

## Contract for T025 — Map-point novelty test uses a fixture past the warm-up
USAGE      `cargo test -p worklog-core daemon_stats` green again after T023.
CALLS      T023 made blocks with < 20 earlier blocks "warm-up" (MapPoint.novelty = 0.0, excluded from first_times) — an intended spec change. `map_points_carry_label_topic_novelty_and_oddball_from_the_blocks` (daemon_stats_test.rs ~:553) asserts `by_id(1).novelty == 1.0` on a 10-block fixture and `by_id(4).novelty < 0.5`, which now passes only vacuously. Update it so it still proves novelty is carried from the blocks: seed ≥ 20 earlier history blocks (or a 25-block fixture), then assert a post-warm-up block that is unlike everything earlier has novelty > 0.9 and one that repeats an earlier block has novelty < 0.1; additionally assert a warm-up block has novelty exactly 0.0. Keep the label / topic / oddball assertions unchanged.
TESTS      the updated test passes; mutate MapPoint novelty to always 0.0 in a scratch copy → the test fails (report it), revert.
THE FIVE   (same five as T015)
