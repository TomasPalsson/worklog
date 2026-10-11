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

## Contract for T026 — Galaxy design pass
USAGE      `<TopicGalaxy report={report}/>`
CALLS      Shared rules for T026–T029 (from the design evaluator, composite 3.15): one card style for every atlas card = the page's `.sr-card` with a `.sr-title` small-caps title (Galaxy title "Topic galaxy", River "Topic river", Mirror "Day mirror", Compass "Day compass", Moments keep theirs); no visible text below 12px, and small secondary text uses `--fg-muted` not `--fg-subtle`; captions in plain words, ≤ 70 characters per line (`max-width: 70ch`), start "How it works:" not "How:"; jargon (t-SNE, perplexity, average-linkage, cosine) moves into a `<details><summary>Method</summary>…</details>` under the caption — visible text says "similarity" not "cosine"; caps text tracking +0.06em. Galaxy specifics: (1) phone — below 600px wrap the SVG in a horizontal scroll container (`overflow-x: auto`, SVG `min-width: 720px`) AND render a colour-keyed topic list under the map (swatch + name + hours, each a focusable row with the label tip) so the map is readable or skippable on a phone; (2) height — compute the viewBox height from the points' y extent (map y range × the existing scale + PAD top/bottom) instead of the fixed 620, min 360; (3) in-card legend row under the map: "dot size = hours · ✦ first time · ◌ oddball" using `FirstTimeMark`/`OddballMark` from AtlasArt and a two-size dot sample; (4) topic name text fill = the topic hue (oklch(var(--project-l) var(--project-c) h) lifted toward --fg with color-mix 35% for contrast), halo stroke `--bg-sunk` 4px with paint-order stroke; (5) each dot gets an invisible hit circle r=12 (pointer target) behind the visible dot.
TESTS      viewBox height < 620 for a fixture whose y range is 0.2–0.6; legend row present with both marks; name fill uses the topic hue; every dot has a sibling hit circle r=12; topic list rendered (CSS hides it ≥600px — assert the class); caption has no "t-SNE" outside <details>.
THE FIVE   (same five as T015)

## Contract for T027 — Heartbeats design pass
USAGE      `<TopicHeartbeats report={report}/>`
CALLS      Shared rules for T026–T029 (from the design evaluator, composite 3.15): one card style for every atlas card = the page's `.sr-card` with a `.sr-title` small-caps title (Galaxy title "Topic galaxy", River "Topic river", Mirror "Day mirror", Compass "Day compass", Moments keep theirs); no visible text below 12px, and small secondary text uses `--fg-muted` not `--fg-subtle`; captions in plain words, ≤ 70 characters per line (`max-width: 70ch`), start "How it works:" not "How:"; jargon (t-SNE, perplexity, average-linkage, cosine) moves into a `<details><summary>Method</summary>…</details>` under the caption — visible text says "similarity" not "cosine"; caps text tracking +0.06em. Heartbeats specifics: (1) the day-tick raster and the hours bar share the same left and right edge in column 3 (remove `.hb-tick { max-width: 8px }`; raster uses `display:grid; grid-template-columns: repeat(N, 1fr)` with N = bins, gap 1px, full width); (2) a first/last date caption under the raster of the first row only ("Sep 23" … "Oct 10", 12px `--fg-muted`); (3) ticket keys 12px `--fg-muted`.
TESTS      raster element uses grid with N columns equal to river bins; first row has the date caption with the trimmed-first and last non-zero bin dates; no inline max-width on ticks.
THE FIVE   (same five as T015)

## Contract for T028 — Mirror + compass design pass
USAGE      `<DayMirror/>`, `<DayCompass/>` inside `.stats-two`.
CALLS      Shared rules for T026–T029 (from the design evaluator, composite 3.15): one card style for every atlas card = the page's `.sr-card` with a `.sr-title` small-caps title (Galaxy title "Topic galaxy", River "Topic river", Mirror "Day mirror", Compass "Day compass", Moments keep theirs); no visible text below 12px, and small secondary text uses `--fg-muted` not `--fg-subtle`; captions in plain words, ≤ 70 characters per line (`max-width: 70ch`), start "How it works:" not "How:"; jargon (t-SNE, perplexity, average-linkage, cosine) moves into a `<details><summary>Method</summary>…</details>` under the caption — visible text says "similarity" not "cosine"; caps text tracking +0.06em. Mirror: (1) legend row "alike ▮ · opposite ▮" with the two colour swatches and a 3-step intensity strip; (2) callout line above the matrix: "Most alike days: <day> & <day> (+0.53) · Most opposite: <day> & <day> (−0.52)" computed from day_similarity (off-diagonal max/min); (3) cell fill contrast: positive cells mix --amber up to 85% at max |sim|, negative --slate up to 85% (was muddier); (4) diagonal labels: never wrap — use one line "Sep 23" at 11px mono... (minimum 12px rule: use row-header labels whenever the cell is < 44px). Compass: (1) a hue key under the chart: "dot colour = that day's main topic" with up to 5 swatches+names of topics that appear; (2) plain axis captions: x "focused → scattered" becomes "scattered ← → focused", y "familiar ↓ ↑ new ground" as small axis titles; (3) a one-line callout: "Most focused: <day> · Most new ground: <day>".
TESTS      mirror callout names the correct most-alike and most-opposite pair for a fixture; legend present; compass key lists only topics that are some day's top topic; compass callout days correct.
THE FIVE   (same five as T015)

## Contract for T029 — River + moments design pass
USAGE      `<TopicRiver/>`, `<TopicMoments/>`
CALLS      Shared rules for T026–T029 (from the design evaluator, composite 3.15): one card style for every atlas card = the page's `.sr-card` with a `.sr-title` small-caps title (Galaxy title "Topic galaxy", River "Topic river", Mirror "Day mirror", Compass "Day compass", Moments keep theirs); no visible text below 12px, and small secondary text uses `--fg-muted` not `--fg-subtle`; captions in plain words, ≤ 70 characters per line (`max-width: 70ch`), start "How it works:" not "How:"; jargon (t-SNE, perplexity, average-linkage, cosine) moves into a `<details><summary>Method</summary>…</details>` under the caption — visible text says "similarity" not "cosine"; caps text tracking +0.06em. River: (1) a faint bottom axis with a tick + "Sep 29"-style label at each Monday (day bins) or every 4th week (week bins); (2) an hours scale: a small vertical bracket at the right edge showing the height of the tallest stack with its hours label (e.g. "8h"); (3) the legend under the chart lists ALL topics (swatch + name), inline labels stay as placed; (4) label font size constant (no 18px jump; on narrow screens the SVG scrolls horizontally with min-width 720px instead). Moments: (1) `.tm-label` two-line clamp with ellipsis (`-webkit-line-clamp: 2`); (2) strip endpoints labelled with the range's first and last day; (3) oddball score caption renamed "closest match 0.23 (typical 0.57)" and the card title subtitle "lower = more unusual"; first-times subtitle "higher = newer ground".
TESTS      river: one axis tick per Monday in a 3-week fixture; legend contains every topic; scale label equals the tallest stack hours. Moments: subtitle strings present; endpoints labelled; label element has the clamp class.
THE FIVE   (same five as T015)

## Contract for T030 — Heartbeats: date range separator, description tier
USAGE      `<TopicHeartbeats/>`
CALLS      Design evaluator round 2 (composite 3.43): the first-row date caption renders "Sep 23Oct 10" jammed against "22h 44m". Render it as its own line under the raster: `<span>Sep 23</span><span aria-hidden="true"> – </span><span>Oct 10</span>`, `display:flex; justify-content:space-between` so the two dates sit at the raster's left and right edges, 12px --fg-muted, with 4px top margin; the hours figure stays on its own line above the bar. Description text 13.5px (one tier above the 12px captions) so the ledger reads first.
TESTS      date caption contains an en dash separator element and both dates as separate spans; hours element is not inside the date caption; description font-size rule 13.5px present.
THE FIVE   (same five as T015)

## Contract for T031 — Galaxy fits the phone; list carries names there
USAGE      `<TopicGalaxy/>`
CALLS      Round 2: at 375px the map is cropped inside a horizontal scroll and reads as broken. Remove the min-width/scroll behaviour; the SVG is always width:100% with `preserveAspectRatio="xMidYMid meet"`; below 600px hide on-map name/hours labels (`.sx-gal-label {display:none}`) and show the existing topic list as the legend (it already has focusable tipped rows); also below 600px enlarge visible dots ×1.6 via CSS transform on the dot group so they stay visible. Keep everything else.
TESTS      no element with min-width 720 / overflow-x scroll in the galaxy; svg has preserveAspectRatio meet; CSS has the <600px rule hiding .sx-gal-label and showing .sx-gal-list.
THE FIVE   (same five as T015)

## Contract for T032 — River labels inside the plot
USAGE      `<TopicRiver/>`
CALLS      Round 2 screenshot: "TINN INFRASTRUCTURE" is clipped at the left edge (label anchored text-middle at a bin near x=0) and "SJÚKRA CLAUDE INTEGRATION" touches the label above it. In placement: clamp label centre x to [box.w/2 + 8, W − box.w/2 − 8] using the same width estimate used for overlap (so SVG text-anchor middle never crosses the plot edges); after clamping, re-check overlap with placed labels (4px pad, now also vertical: boxes must be separated by ≥ 4px in y when they overlap in x); a label whose band is < 18px thick at its clamped x is dropped to the legend. Use the measured per-char width 7.2px at 12px mono for the estimate (was 6.6).
TESTS      a topic whose widest bin is the first bin gets a label box with x0 ≥ 8; crowded fixture: all placed boxes pairwise separated (x or y) by ≥ 4px; band 17px → legend only.
THE FIVE   (same five as T015)

## Contract for T033 — Plain words on moments + compass; mirror readable and reachable
USAGE      `<TopicMoments/>`, `<DayCompass/>`, `<DayMirror/>`
CALLS      Round 2 critiques, plain-language and a11y floor (accessibility 3.0, needs ≥ 3.5). Moments: put a word beside each big number — first times: "0.85 new" (number + small "new" label), oddballs: "0.23 match" with subtitle "lower = more unusual" kept; remove "Nearest anything"/"cosine"/"novelty" from visible text and tips (tip rows "Newness", "Closest match"). Compass: replace the four quadrant captions with two axis titles only — x "Focus →" (left "scattered", right "focused" small end labels), y "Newness ↑" (bottom "familiar", top "new ground"); give dots a 1.5px --bg-raised stroke so overlapping dots separate. Mirror: off-diagonal cells get `tabIndex={0}` only for the upper-triangle cells of the LAST 14 days (bounded tab stops ≤ 105) plus aria-label "Oct 5 and Oct 7: 0.53 alike" (or "opposite"); draw a 2px --fg ring around the callout's most-alike cell and a dashed ring around the most-opposite cell; replace "alike/opposite" legend wording with "similar work / different work".
TESTS      moments big-number has the word label; no "cosine"/"novelty"/"Nearest anything" in rendered text or tips; compass has exactly two axis titles and no quadrant captions; dots have the stroke attr/class; mirror: tab stops ≤ D + 105, a cell aria-label matches the pattern, ring elements present on the right cells.
THE FIVE   (same five as T015)

## Contract for T034 — Mirror: tab stops only on day chips and the two ringed cells; plain callout
USAGE      `<DayMirror/>`
CALLS      Design round 3 (3.43; usability/a11y 3.0): even the bounded 14-day cell window is a keyboard flood. Off-diagonal cells: NO tabIndex except the two ringed cells (most alike, most opposite), which get tabIndex 0 + their aria-label; every cell keeps data-stip for pointer/touch. Diagonal day chips stay the per-day tab stop (their tip already names most/least like). Callout in words first: "Most similar days: Oct 5 & Oct 7 · Most different: Sep 23 & Oct 5" (numbers only in the ringed cells' tips). All visible text at least 13px.
TESTS      tab stops == D + 2; ringed cells focusable with aria-label; callout shows no numbers other than the dates; no visible text rule below 13px in mirror.css.
THE FIVE   (same five as T015)

## Contract for T035 — Galaxy: phone tap targets, min height, label spacing
USAGE      `<TopicGalaxy/>`
CALLS      Round 3: at 375px the map is ~160px tall and the r=12 hit circle is ~4px on screen; labels "CODE INTERPRETER INFRASTRUCTURE" / "CODE INTERPRETER AND HISTORY CONFIGURATION" stack with no gap. (1) below 600px: `.sx-gal-hit` scaled x3 (transform-box fill-box, origin center) and the map container min-height 260px (svg height 100%, preserveAspectRatio meet keeps it centred); (2) label layout: a label's collision box includes its hours line (h = font + 4 + 12) and the nudge pad becomes 8px; (3) all visible caption/legend text at least 13px.
TESTS      CSS has the x3 hit scale and 260px min-height under 600px; two labels at the same median y end at least 8px apart including the hours line; legend/caption font rules at least 13px.
THE FIVE   (same five as T015)

## Contract for T036 — Heartbeats: 13px type floor, meaningful keyword chips
USAGE      `<TopicHeartbeats/>`
CALLS      Round 3: generic chips ("tool", "code", "block", "pull", "enable", "pages", "chats") add noise; 12px muted text dominates. (1) chips: drop keywords in a small local list of generic dev words (tool, tools, code, block, blocks, pull, enable, enabled, page, pages, chat, chats, project, projects, update, fix, issue, issues, test, tests, config, configuration, setup, work) and show up to 4 remaining; (2) every visible text rule at least 13px (chips, tickets, badge, dates, how); (3) date range caption on every row (its own first/last active day for that topic from the river), not only the first row.
TESTS      a topic with keywords [code, librechat, tool, pptx] shows only librechat and pptx; no font-size under 13px in heartbeats.css; every row has a date caption with its own first/last active days.
THE FIVE   (same five as T015)

## Contract for T037 — Moments words first, one axis per column; compass labels and axis text
USAGE      `<TopicMoments/>`, `<DayCompass/>`
CALLS      Round 3. Moments: lead each card with words, number second — first times: "New ground" badge + "0.85" small; oddballs: "Unlike anything else" badge + "closest 0.23 · usual 0.57" small; tips keep the numbers; remove the per-card strip end labels and show "Sep 23 … Oct 10" once under each column heading; all text at least 13px. Compass: label up to 5 dots (the extreme per quadrant plus the most-hours day) with the existing leader/clamp placement; vertical axis end labels horizontal (no rotate) at top-left/bottom-left; all text at least 13px.
TESTS      moments: badge words present and precede numbers; exactly one strip-axis caption per column; compass: label count min(5, plotted); no rotate transform on axis end labels; no font-size under 13px in moments.css/compass.css.
THE FIVE   (same five as T015)

## Contract for T038 — Retired-card guard checks headings, not prose
USAGE      `cd web && bun test components/stats/TopicsSection.test.tsx` green.
CALLS      `output carries no retired drift/sprawl/similar-days text` (TopicsSection.test.tsx ~:164) bans the substring "similar days" anywhere; T034's new mirror callout legitimately reads "Most similar days: Oct 1 & Oct 2", so the guard now false-positives. Keep the guard's intent (the retired cards must not come back): assert that no heading / card title element (h2, h3, .sr-title) has text equal (case-insensitive, trimmed) to any of "Switches per day", "Drift", "Catch-all tickets", "Similar days", and that the rendered text contains none of "drift", "sprawl", "similar_days", "catch-all" (drop only the plain "similar days" substring from the prose check). Prove it bites: render a scratch element with an h3 "Similar days" inside the test's container in a temporary local test → fails; remove the scratch.
TESTS      the updated guard passes on the current output and fails when a retired heading is present.
THE FIVE   (same five as T015)

## Contract for T039 — Review fixes: skipped topics back off, name length limits, layout off the DB lock, dead lookup gone
USAGE      GET /stats/topics behaves as before; naming never loops on a topic the model keeps skipping.
CALLS      Whole-branch review (blind re-score >= 80 kept): (1) [80, verified] `parse_reply` returns Ok when at least one requested id is named; omitted ids are silently dropped, `name_batch` returns Ok, `record_failure` is never called for them, so the next GET re-claims and re-asks forever. Fix: after a successful batch, call `record_failure` for every requested topic id the reply did not name (they enter the normal 10-minute back-off). (2) [80, advisory] design.md promises name <= 40 chars and description <= 200 chars else keyword fallback for that id — enforce it in `parse_reply`: a name longer than 40 chars (after trim) is treated as unnamed for that id (keyword fallback + back-off via (1)); a description longer than 200 chars is truncated at a word boundary to <= 200 with "…". (3) [75, low impact but cheap] `topics_finish` runs `topic_map::layout` (O(n^2)×750) inside `with_conn`, holding the single shared DB lock: move the pure computation (build_with_assignments, keywords, layout) into the plain spawn_blocking phase with no connection held, keeping only the topic_names reads/writes inside `with_conn` (same three-phase pattern already used for /embed). (4) [78] `TopicsInput.ticket_summaries` is filled per request but never read: delete the field, its init and the per-ticket query.
TESTS      a reply naming 3 of 4 → the 4th is backed off (next GET does not invoke for it) while the 3 are stored; a 41-char name → keyword fallback for that id and backed off, a 40-char name accepted; a 250-char description stored truncated <= 200 ending "…"; route still Ready with points; existing tests green; `grep ticket_summaries` finds nothing.
THE FIVE   (same five as T015)

## Contract for T040 — Final a11y pass: phone text and targets, ticket keys, light-mode marks, percentages
USAGE      Topic atlas on /stats, all views.
CALLS      Design round 4 (composite 3.38; usability/a11y 3.0). Apply exactly these: (1) compass.css: `@media (max-width: 599px) { .sx-cmp-end, .sx-cmp-axis, .sx-cmp-leader-label (whatever the leader label class is in DayCompass.tsx) { font-size: 22px; } }` so viewBox text renders ~12px at 343px. (2) mirror.css: below 599px the grid sits in an `overflow-x: auto` wrapper and each column is at least 24px wide (min-width on the grid = 24px × columns); if the wrapper element does not exist, add it via CSS only on the existing card container. (3) River: `GUTTER` 48 → 60 (scale label no longer clipped); at ≤ 599px add a right-edge fade mask on the scroll container (`mask-image: linear-gradient(to right, black 85%, transparent)`) and a small "scroll →" caption. (4) Heartbeats: each ticket key in its own `<span class="hb-ticket">` with `white-space: nowrap`, the ticket line prefixed "Tickets:"; extend the local GENERIC chip list with: pptx, prs, pr, file, files, boost, thinking, binding, multi, base, revise, pdf. (5) Light-mode mark contrast: in galaxy.css (it is imported by the atlas section) add `:root:not([data-theme="dark"]) .sx-gal, :root:not([data-theme="dark"]) .sx-river, :root:not([data-theme="dark"]) .hb-row, :root:not([data-theme="dark"]) .sx-cmp { --project-l: 0.60; }` guarded so it only applies in light mode (respect both dark selectors: under `@media (prefers-color-scheme: dark) :root:not([data-theme="light"])` and `:root[data-theme="dark"]` it must NOT apply). (6) Moments: show scores as percentages — first times "85% new", oddballs "closest match 23% · your usual 57%"; tips keep the decimals.
TESTS      update/extend the existing stats tests for (3) gutter/caption, (4) nowrap ticket spans, prefix and new generic words filtered, (6) percentage strings; a CSS assertion for (1), (2) and (5) rules; full `bun test components/stats` green and typecheck clean.
THE FIVE   (same five as T015)

## Contract for T041 — Build lint: keys on JSX arrays in AtlasArt test
USAGE      `cd web && bun run build` succeeds.
CALLS      `next build` lint fails: `./components/stats/AtlasArt.test.tsx` lines ~10-17 and ~32: "Missing "key" prop for element in array (react/jsx-key)". Give every JSX element inside those array literals a stable `key` (e.g. the kind name). Do not change any assertion. Do not disable the lint rule.
TESTS      AtlasArt tests unchanged and green; `bun run build` exit 0.
THE FIVE   (same five as T015)

## Contract for T042 — Moments percentages stay within 0–100% and match tips/labels
USAGE      `<TopicMoments/>`
CALLS      Review of T040: novelty is `1 − best cosine` on centred vectors, so it ranges 0..2, and nearest cosine can be negative; `pct(score)` then shows "150% new" or "closest match -50%". Map scores to a bounded 0–100% before display: newness% = clamp(novelty / 2 × 100... — NO: use newness% = round(clamp(novelty, 0, 1) × 100) and match% = round(clamp(cosine, 0, 1) × 100) (a negative cosine reads as 0% match, i.e. "nothing alike"); apply the same mapping to the median. Use the same percentage strings in the card's aria-label, the tip rows and the How line, so every surface shows one unit.
TESTS      novelty 1.5 → "100% new"; cosine −0.5 → "closest match 0%"; median −0.1 → "your usual 0%"; aria-label and tip rows contain the same percentage strings as the visible card.
THE FIVE   (same five as T015)

## Contract for T043 — Namer is told the 40-char name limit
USAGE      the naming job's model call.
CALLS      Review of T039: `parse_reply` drops names > 40 chars, but `SYSTEM_PROMPT` and `schema()` never state the limit, so a model that keeps returning long names gets re-asked every back-off window forever. Add `"maxLength": 40` to the name property and `"maxLength": 200` to description in `schema()`, and one prompt line: "Each name must be at most 40 characters." Keep everything else.
TESTS      schema JSON contains maxLength 40 for name and 200 for description; SYSTEM_PROMPT contains the 40-character sentence.
THE FIVE   (same five as T015)

## Contract for T044 — One shared topic colour rule with alternating lightness
USAGE      `import { topicColor } from "./topicColor"; style={{ fill: topicColor(t.id, topics.length) }}`
CALLS      Design evaluator: ~10 topic hues where neighbours (teal/green/blue) are hard to tell apart; the hue rule is copy-pasted in 5+ modules (Discovered in NOTES). Create `web/components/stats/topicColor.ts` exporting `topicHue(id, k): number` (= 25 + id*(360/k), unchanged) and `topicColor(id, k): string` returning `oklch(calc(var(--project-l) + ${id % 2 === 0 ? 0 : -0.08}) var(--project-c) ${hue})` — odd ids are 0.08 darker so adjacent hues separate by lightness too. Replace every local hue/colour helper in the six modules with these imports (same visual hue, now with alternation). Do not change anything else.
TESTS      topicColor.test.ts: hue for id 3 of 10 = 133; even ids have no lightness offset, odd ids −0.08; each module file no longer defines its own hue function (grep-style test reading the source files for `function topicHue` / `const hue =` definitions).
THE FIVE   (same five as T015)

## Contract for T045 — Galaxy headline insight and dot hint
USAGE      `<TopicGalaxy/>`
CALLS      Design evaluator: the galaxy has no "so what". Add one headline line above the map (16px, --fg): "{K} topics · {A} and {B} are {P}% of your time" where A, B are the two largest topics by seconds and P = their combined share of all topic seconds rounded; with one topic: "1 topic · {A}". Append to the legend row: "· hover or tap a dot for the block". Keep all else.
TESTS      headline text for a 3-topic fixture with known seconds; one-topic fixture wording; legend contains the hint.
THE FIVE   (same five as T015)

## Contract for T046 — Galaxy labels never collide and topic list is keyboard-reachable on desktop
USAGE      `galaxyLayout(report)` → labels; `<TopicGalaxy/>`
CALLS      Design evaluator round 5 (composite 3.43): on real data the label box for "Code Interpreter and History Configuration / 2.0h" overprints "Sjúkra Claude Integration / 4.2h"; long names reach the right edge; per-block data is keyboard-unreachable because `.sx-gal-list` is `display:none` above 599px. In `web/components/stats/TopicGalaxy.tsx` `galaxyLayout` (line ~68–115):
  1. Add `const MAX_LABEL_CHARS = 24;` — label text longer than 24 chars is cut to 23 chars + "…" (cut at the last space ≤ 23 when one exists after char 10, then "…"). The full name stays in the label's aria-label and tooltip (unchanged code paths using `topic.name`).
  2. The label box used by the overlap nudge must cover BOTH lines (name line + hours line): `h` = fontSize + LABEL_LINE_GAP + the hours line's font size (read how the hours line is rendered ~line 217–230 and use its real size), and `w` = max(name width, hours width).
  3. After nudging, re-clamp x with the label's real `w` so `x + w/2 <= LABEL_MAX_X` and `x - w/2 >= LABEL_MIN_X`; re-clamp y to the map's vertical bounds the same way.
  4. Run the existing pairwise nudge until no two label boxes overlap (with LABEL_GAP) or 50 passes, whichever first.
  In `web/app/stats/galaxy.css`: replace `.sx-gal-list { display: none; …}` at desktop with a visually-hidden-but-focusable pattern (position:absolute; width:1px; height:1px; overflow:hidden; clip-path: inset(50%); white-space:nowrap) that becomes fully visible (static, auto size, no clip) on `:focus-within` and at ≤599px (keep the existing phone rule). Do not change dot `tabIndex`.
TESTS      (a) a 10-topic fixture whose labels would collide (two topics with near-identical centroid and long names) → no two label boxes overlap, every box inside [LABEL_MIN_X, LABEL_MAX_X] horizontally; (b) a 42-char name renders ≤ 24 chars ending in "…" while its aria-label carries the full name; (c) label `h` exceeds fontSize + LABEL_LINE_GAP (hours line counted); (d) galaxy.css has no `display: none` on `.sx-gal-list` outside the ≤599px media query and contains `:focus-within`.
THE FIVE   (same five as T015)

## Contract for T047 — Heartbeats compact rows, river legend removed, river scale says per day
USAGE      `<TopicHeartbeats/>`, `<TopicRiver/>`
CALLS      Design evaluator round 5: the ten topics are listed four times; Heartbeats takes ~1000px; river scale "10.4h" lacks a unit.
  TopicHeartbeats.tsx / heartbeats.css: description clamped to ONE line (`-webkit-line-clamp: 1` + `title` attribute with the full text, also keep it in the row's aria-label if one exists); keyword chips capped at 3 (`MAX_CHIPS = 3`); ticket chips moved out of the visible row into the row's tooltip — the row already uses tipProps or add `{...tipProps(...)}` from `./StatTip` with rows ["Tickets", labels joined by ", "] alongside whatever the row tip has (grep StatTip usage in the same folder for the exact API); row vertical padding ≤ 8px so a row is ≤ ~64px at 1280px. Keep the rhythm badge, raster, hours bar.
  TopicRiver.tsx / river.css: delete the `<ul className="sx-river-legend">` block and its CSS (Heartbeats is the colour key now). Topics that had no in-band label must still be discoverable: their band keeps its aria-label/tooltip (unchanged). The scale bracket label reads `${scaleHours(x)} / day` when bin is "day" and `${scaleHours(x)} / week` when bin is "week" (find where scaleHours is rendered).
TESTS      Heartbeats: ≤3 chips for a topic with 8 keywords; no `.hb-ticket` in the visible row but the tooltip payload contains the ticket labels; description element carries `title` = full text; CSS has line-clamp 1. River: no `.sx-river-legend` in markup or river.css; scale label ends "/ day" for bin day and "/ week" for bin week.
THE FIVE   (same five as T015)

## Contract for T048 — Galaxy names wrap to two lines instead of being cut
USAGE      `galaxyLayout(report)`; `<TopicGalaxy/>`
CALLS      Design evaluator round 6: five of ten galaxy names end in "…" (T046's `shortName`, TopicGalaxy.tsx:69) and similar names ("Code Interpreter Infrastructure" vs "Code Interpreter and History Configuration") become indistinguishable. Replace the cut with a wrap: add `export function wrapName(name: string): string[]` — names ≤ MAX_LABEL_CHARS (24) → `[name]`; longer → split at the last space at or before index 24 into two lines; if there is no such space, or the second line is still > 24 chars, the second line is cut to 23 chars + "…" (so at most two lines, each ≤ 24 chars, and the first line is always whole words). Render each line as a `<tspan x={l.x} dy=…>` inside the existing `.sx-gal-name` text (first line dy 0, second line dy = fontSize * 1.1). The hours line moves down by one extra name line when wrapped. The label box used by the nudge: `h = lines * fontSize * 1.1 + LABEL_LINE_GAP + HOURS_FONT` (adjust to the real render offsets you use), `w = max(widest name line, hours width)`. Keep the pairwise nudge, clamping and the 50-pass cap from T046. Remove `shortName` if nothing else uses it. The aria-label and tooltip keep the full `topic.name`.
TESTS      wrapName: 24-char name → 1 line; "Code Interpreter and History Configuration" → ["Code Interpreter and", "History Configuration"]; a 60-char name with spaces → 2 lines, second ends "…", each ≤ 24; a 30-char name with no space → 2 lines… (first 24 chars, then rest) — define and assert your rule. Layout: a wrapped label's h is greater than an unwrapped one at the same fontSize; the 10-topic collision fixture from T046 still has no overlapping boxes. Render: the two-line label renders two tspans and no "…" for "Code Interpreter and History Configuration".
THE FIVE   (same five as T015)

## Contract for T049 — Mirror column date labels and full compass key
USAGE      `<DayMirror/>`, `<DayCompass/>`
CALLS      Design evaluator rounds 5 and 6: the Mirror staircase has row labels (`.sx-mir-rowlabel`, DayMirror.tsx:73) but no column labels, so the second day of a pair cannot be found; the Compass key shows only KEY_MAX topics (DayCompass.tsx:121) and dot size is unexplained.
  DayMirror: add a column-label row above the grid — one `<span className="sx-mir-collabel" aria-hidden="true">` per day placed with `gridRow` of a new first row and `gridColumn: column + 2` (match how row labels/cells are positioned; shift existing rows down by one if the grid uses explicit gridRow), text = day-of-month number (e.g. "23"), shown on every column when there are ≤ 31 days and every other column otherwise. Do NOT add tab stops (keep the current tabIndex model unchanged). mirror.css: `.sx-mir-collabel` 11px mono, --fg-muted, centered.
  DayCompass: key lists every topic that has at least one plotted day (drop the KEY_MAX slice), sorted as today; append a key item "dot size = hours that day" (or what the dot radius actually encodes — read the code and state it exactly; if radius is constant, write nothing about size).
TESTS      Mirror: a 10-day fixture renders 10 `.sx-mir-collabel` with the day numbers; a 40-day fixture renders 20; the count of tabIndex=0 elements is unchanged from before the change (assert the exact number for the fixture). Compass: a fixture with 7 topics plotted renders 7 key swatches; the size note text matches the real radius encoding.
THE FIVE   (same five as T015)

## Contract for T050 — Atlas type hierarchy: names larger and sans, mono for numbers only
USAGE      CSS only
CALLS      Design evaluator rounds 5 and 6: everything is 13px mono, so there is no hierarchy. heartbeats.css: `.hb-name` → `font-family: inherit` (Geist sans), `font-size: 15px`, `font-weight: 600`, `line-height: 1.25`; `.hb-chip` stays mono 12px; `.hb-description` 13px. moments.css: `.tm-label` 14px weight 500; `.tm-num`, `.tm-when` stay mono but 12px; `.tm-chip` 12px. Change nothing else.
TESTS      CSS-content tests (the existing pattern in these test files reads the css file): .hb-name has 15px, 600 and no var(--font-mono); .tm-label 14px; .tm-num 12px mono.
THE FIVE   (same five as T015)

## Contract for T051 — Heartbeat strips share one time axis
USAGE      CSS only
CALLS      Design evaluator round 7 (severity 3, chart honesty): heartbeat rasters end at different x per row (855/865/868/889 px) so the same date sits at a different x in each row. Cause: `.hb-track` also has class `.sr-track`, whose rule `grid-column: 2 / 4` (web/app/stats/charts-ranked.css:12) spawns implicit auto columns inside the single-column `.hb-data` grid (heartbeats.css:23), so column 1 width depends on each row's text. Fix in web/app/stats/heartbeats.css only: give `.hb-raster`, `.hb-dates`, `.hb-track` and `.hb-hours` (every direct child of `.hb-data`) `grid-column: 1 / -1` (or `1`) so `.hb-data` never grows implicit columns; keep `grid-template-columns: minmax(0, 1fr)`. Do not touch charts-ranked.css (shared by other charts). If the hours/dates line needs to sit on one row, keep it inside `.hb-dates` as today.
TESTS      CSS-content test: heartbeats.css sets `grid-column` on `.hb-track` (overriding sr-track) and on `.hb-raster`; a render test that every `.hb-data` child has no inline gridColumn other than column 1 is optional. Keep the 13px floor test green.
THE FIVE   (same five as T015)

## Contract for T052 — River focus ring and 13px text
USAGE      CSS + one constant
CALLS      Design evaluator round 7: `.sx-river-band:focus-visible { outline: none }` (river.css:2) leaves only an opacity change as the focus cue; `.sx-river-name` and `.sx-river .st-svg .art-label` are 12px, under the page's 13px floor; the scale label "10.4h / day" may clip in the 60px gutter (TopicRiver.tsx:12 `GUTTER = 60`). Fix: hover keeps fill-opacity 1; `:focus-visible` gets `fill-opacity: 1; stroke: var(--fg); stroke-width: 2` (no `outline: none` on focus); both 12px rules → 13px; `GUTTER` → 96. If any label-fit logic in TopicRiver.tsx uses a char-width constant tied to 12px, scale it to 13px so overlap checks stay correct.
TESTS      river.css: no `outline: none` on focus-visible; focus-visible has a stroke; no font-size under 13px (add a floor test like the other modules'); GUTTER = 96 reflected in viewBox width (W + 96). Existing label-placement tests stay green (update only numbers that follow mechanically from the 13px/char width change and say which).
THE FIVE   (same five as T015)

## Contract for T053 — One name per idea: First time / Oddball / Newness
USAGE      copy only
CALLS      Design evaluator round 7: one idea has several names — "First times" (card), "first time" (galaxy legend), "New ground" (moments badge), "Novelty" (galaxy dot tip, TopicGalaxy.tsx:162); "Oddballs" (card), "oddball" (legend), "Unlike anything else" (badge). Rule: badge text in TopicMoments.tsx:54 becomes "First time" / "Oddball"; galaxy dot tip row "Novelty" → "Newness" (matching the compass axis). Column titles "First times"/"Oddballs" and the legend stay. Change no other copy.
TESTS      update the assertions that pin the old strings to the new ones (this is the requested change, not a weakening); add one assertion that no rendered Moments/Galaxy output contains "New ground", "Unlike anything else" or "Novelty".
THE FIVE   (same five as T015)

## Contract for T054 — Three honesty fixes: compass arrow, river clipping, heartbeat active-dates label
USAGE      `<DayCompass/>`, `<TopicRiver/>`, `<TopicHeartbeats/>`
CALLS      Design evaluator round 8 found three things that mislead on real data:
  1. DayCompass.tsx:136 — the y-axis label "Newness ↑" sits in a `<text>` with `rotate(-90)`, so the glyph rotates and renders pointing LEFT, while "new ground" is at the TOP. Change the text to "Newness →" (under rotate(-90), "→" points up).
  2. TopicRiver.tsx:17 `CHAR_PX = 7.8` ignores river.css's `letter-spacing: 0.06em` on 13px mono uppercase (real advance ≈ 13·0.6 + 13·0.06 = 8.58); labels like "CODE INTERPRETER INFRASTRUCTURE" clip at the left edge and the scale "10.4h / day" clips to "10.4h / da". Set `CHAR_PX = 8.6` and `GUTTER = 120` (line 12). Comment the 8.6 derivation in one line.
  3. TopicHeartbeats.tsx ~114–118 + heartbeats.css:25 — `.hb-dates` pins the topic's first-active and last-active dates to the left and right ends of a raster that spans the WHOLE report range, so "Sep 23 – Oct 1" looks like the raster's own edges. Render the dates as one left-aligned line "active Sep 23 – Oct 1" (one span of text; same shortMonthDay calls; when both dates are equal: "active Sep 23"), and change `.hb-dates` to `justify-content: flex-start` (keep 13px, --fg-muted, margin).
TESTS      Compass: the rotated axis text is "Newness →" (assert no "↑" inside the rotated element). River: CHAR_PX = 8.6 reflected in labelBox width (chars × 8.6); viewBox width = W + 120. Update the existing numeric assertions that follow mechanically (say which). Heartbeats: the dates element's text is "active Sep 23 – Oct 1" for a two-date fixture and "active Sep 23" for a same-day fixture; css has `justify-content: flex-start` on `.hb-dates`. Keep all 13px floor tests green.
THE FIVE   (same five as T015)
