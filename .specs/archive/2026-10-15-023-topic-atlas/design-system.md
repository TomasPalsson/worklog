DESIGN BRIEF: worklog Statistics page. All paths are relative to `/Users/tomas/Desktop/Projects/worklog/.claude/worktrees/embedding-stats/web`. Nothing was modified.

## 1. Page structure
- `app/stats/page.tsx` is a server component with `dynamic = "force-dynamic"` (:32).
- `Section` (:42-49) renders `<section.stats-sec><h2>title</h2>children</section>`.
- The h2 is a mono 11px uppercase label with a bottom hairline (`app/stats/stats.css:23-27`).
- Section order (`Story`, :62-139):
  1. `StatsHero` (no Section wrapper)
  2. Your city (`WorkdayCity`)
  3. Records (`FunFacts`)
  4. Where the time went (`DayFlow`)
  5. When you work (`WeekTerrain` + `WorkdaySpans`)
  6. Every day (`CalendarHeatmap`, `DailyStack`, `ActivityStream`)
  7. Claude & you (`ToolWarehouse`, `PoliteBot`)
  8. Tools of the trade (`ProjectPodium`, `ShellKeycaps`, `TabGarden`, `RankedBars` for Slack)
  9. Tickets (`TicketTimeline`)
  10. **Topics** (:116-120)
  11. Housekeeping (3x `SplitDonut` in `.stats-three`)
  12. Your receipt (`PeriodReceipt`)
- Topics is wrapped in `<Suspense fallback={<p>Loading topics…</p>}>` and the async `Topics` helper (:58-60). `topics = getTopics(from,to).catch(()=>null)` is started before `await getStats` (:148-150), so the page paints first and topics stream in.
- The whole `Story` sits inside one `<TipLayer>` (:169-171). Any new graphic inside a Section gets hover cards for free.
- Layout wrappers:
  - `app/stats/layout.tsx:1-5` imports `stats.css` and renders `div.stats-page`.
  - `.stats-main`: flex column, gap 56px (40px at max-width 480px), `stats.css:5,100`.
  - `.stats-stack`: flex column, gap 28px (:31).
  - `.stats-two` and `.stats-three`: auto-fit grids with 320px and 240px minimums (:28-29).
  - The page is capped at 1100px (:3).

## 2. Data flow
- `getStats(from,to)` is at `lib/daemonStats.ts:~12` and returns `StatsReport`. `getTopics(from,to)` is at :15-21 and calls `GET /stats/topics`.
- The timeout for the topics route is 90s; plain `/stats` is 30s (`lib/daemonTimeout`, asserted in `TopicsSection.test.tsx`).
- All numbers are computed in Rust. The web only draws (header comments in both contract files).
- `StatsReport` (`lib/stats_contract.ts:136+`):
  - `daily: DailyStat[]` (:143)
  - `punchcard: number[][]` (:145)
  - `tickets: TicketStat[]` (:160)
  - `tools`, `folders`, `shell`, `domains`, `records`, `flow_blocks`, `totals`, `sync`
- `Ranked = {label, value}` (:6-9).
- `TopicsReport` (`lib/topics_contract.ts`):
  - `status: "ready" | "helper_down" | "too_few_blocks"`
  - `topics: Topic[]`, with `{id (0-based, by seconds desc), name, name_ticket, seconds, blocks, top_tickets: Ranked[]}`
  - `switches: DaySwitches[]`, with `{day, switches, work_seconds, longest_run_seconds}`
  - `drift: TopicDrift[]`, with `{topic_id, name, seconds_now, seconds_before}`
  - `sprawl: TicketSprawl[]`, with `{ticket, summary|null, blocks, spread (1 - mean pairwise cosine; 0 = alike)}`
  - `similar_days: SimilarDay[]`, with `{day, similarity 0..1, work_seconds, top_tickets: Ranked[]}`
  - `embedded_blocks`, `skipped_blocks`
- The contract has no coordinates or vectors. A 2D map, topic-to-topic similarity or per-block data needs a Rust contract change first (`rust/crates/worklog-core/src/topics_contract.rs`).
- Format helpers in `lib/format.ts`: `formatDuration` (:36), `formatTotalHours` (:45), `shortWeekday` (:149), `shortMonthDay` (:159), plus date helpers.

## 3. Current TopicsSection (`components/stats/TopicsSection.tsx`, 64 lines, server-safe, no "use client")
- It handles the null and `helper_down` case (:16-18), where `<p.sr-empty>` reads "Topic stats need the local helper (`worklog verdict`)."
- It handles `too_few_blocks` (:19-21) with "Not enough described work in this range yet."
- The ready state (:22-63) is `div.stats-stack` holding:
  - one real chart: `<RankedBars title="Topics" unit="seconds">`
  - four plain-text `<ul.sr-rows><li.sr-label>` lists inside a local `Card` (:6-13, which reuses `sr-card` and `sr-title`): Switches per day, Drift, Catch-all tickets, Similar days
- The four lists are the places to replace with graphics.
- `components/stats/TopicsSection.test.tsx` asserts exact strings:
  - the two empty messages
  - "2h" and "1h"
  - "5 switches", "1h 30m", "now 2h", "before 30m"
  - "ABC-9", "Misc chores", "0.42", "87%"
  - no "null", NaN or undefined
- If the text changes, update those assertions.

## 4. Component inventory (all in `components/stats`, `.tsx` except where noted; line counts include tip builders)

**SVG and isometric**
- Isometric kit: `iso.ts` (38 lines). It has `iso()`, `isoBox()` (top/left/right polygons), `tones(c)` (colour, 76% and 58% `color-mix` toward black) and `byDepth`. Use it for any 3D piece.
- `WorkdayCity` (330): isometric day-towers with lit windows, using `sx-city-*` classes.
- `WeekTerrain` (328): isometric weekday x hour pillar field from `punchcard`. It uses `sx-terrain-*`, a legend, leader lines and plates.
- `TabGarden` (288): isometric plants (height ~ sqrt of minutes) on soil.
- `ProjectPodium` (279): isometric runway blocks plus deterministic confetti.
- `ShellKeycaps` (245): isometric keycaps.
- `ToolWarehouse` (379): isometric crates.
- `PoliteBot` (358): robot mood art.

**SVG, flat**
- `SplitDonut` (131): `viewBox 0 0 100 100`, R=40, stroke arcs with `pathLength=1` and class `art-draw`. Colours cycle `--sage`, `--amber`, `--violet`, `--slate`, `--terracotta` (`SplitDonut.tsx:8`), and a hatch pattern is used for "none" rows. It has a legend list.
- `DailyStack` (183): stacked bars with a y-axis.
- `ActivityStream` (197): ThemeRiver-style streamgraph. It has monotone-cubic (Fritsch-Carlson) curves, `bandPath` and a centred baseline, so the maths is reusable.
- `WorkdaySpans` (212): candle spans with median lines.
- `CalendarHeatmap` (170): quartile shading, with screen zoom for short ranges.
- `StatsHero` (292): tiles plus a 100x26 polyline sparkline. Its bucketing helper sums a series into at most n buckets.

**CSS and HTML only**
- `RankedBars` (91): `ul.sr-rows` with `.sr-track` and `.sr-fill` bars (CSS). It has a crown SVG on rank 1 and `data-top` amber highlighting.
- `FunFacts` (250), `PeriodReceipt` (281), `TicketTimeline` (197, `"use client"`, CSS grid Gantt), `RangeChips` (39).

**`components/art/*`** is the older "instrument panel" set (SVG): `AllowanceGauge`, `BillableDays`, `CoverageRing` (68), `DayFlow` (382, `"use client"`, carries its own nested TipLayer), `ExportedStamp`, `ExportStrip`, `PruneDial` (151), `RoundingRail`, `RoutingPipeline`, `WeekPace` (55). All are used on the Billing and Day pages, not on Stats. `CoverageRing` and `PruneDial` are the closest gauge and ring candidates.

**Reuse candidates for topics data**
- `SplitDonut` for topic share. Its 5-colour palette repeats after 5, so more than 5 topics needs a new colour scheme.
- `RankedBars` and `rowTip` for the topic ranking, as already used.
- `ActivityStream` curve and band maths for a topic-over-time river. This needs per-day topic data the contract doesn't have.
- `CalendarHeatmap` shading for similar-day cells.
- `WeekTerrain` and `iso.ts` for 3D.
- `StatsHero` sparkline for per-day switches.
- `DailyStack` axis maths (`:19`) for switches per day.

## 5. Hover-card system
- `tip.ts` (51 lines, server-safe).
  - `Tip = {title, sub?, rows?: [k,v][], bar?: {value,max,label?}, note?, accent?}`.
  - `tipProps(t)` returns `{"data-stip": JSON}`. It is spread on any HTML or SVG element.
  - The attribute is deliberately `data-stip`, not `data-tip`, because `globals.css` styles `[data-tip]`. This is covered by a test.
  - `parseTip` and `placeTip(x,y,w,h,vw,vh)` use GAP 14 and flip near the viewport edge.
- `StatTip.tsx` (169, `"use client"`).
  - `TipLayer` handles pointer move/leave/down (touch toggles) and focus/blur. `Escape` and a scroll of more than 4px close the card.
  - Layers nest; the innermost claims the event.
  - Card markup is `.stip-head` (swatch and mono title), `.stip-sub`, `dl.stip-rows`, `.stip-bar`, and `.stip-note` (italic, dashed top border).
- Convention: add `tabIndex={0}` and an `aria-label` to tipped marks. Drop native `<title>` so the grey browser tooltip doesn't double up.
- `chart-tip-utils.ts` exports `nf`, `ordinal`, `rankOf`, `pct`, `times`. Per-component tip builders are exported pure functions (`rowTip`, `segTip`, `cellTip`) plus separate `*Tips.ts` files (`CityTips`, `WarehouseTips`, `PoliteBotTips`).
- `app/stats/tip.css`:
  - `.stip-card` is fixed, z-index 60, `pointer-events: none`, min-width 180px, max-width `min(320px, 100vw-16px)`, padding `10px 12px`, radius `--radius-md`, font 12.5px.
  - Background `--bg-raised`, border `--border-strong`, shadow `--elevation-modal`.
  - Opacity and translateY(4px) transition over 120ms with `--art-ease`.
  - `[data-stip]{cursor:help}` and the focus-visible ring is `2px solid var(--amber)`.

## 6. Tokens (`app/globals.css`)
Light is `:root` (:16-82). Dark is defined twice and must be kept in sync by hand: `@media (prefers-color-scheme: dark) :root:not([data-theme="light"])` (:99-141) and `:root[data-theme="dark"]` (:143+).

| Token | Light | Dark |
|---|---|---|
| `--bg` | `oklch(0.985 0.003 85)` | `oklch(0.15 0.006 85)` |
| `--bg-raised` | `oklch(0.998 0.002 85)` | `oklch(0.18 0.006 85)` |
| `--bg-sunk` | `oklch(0.965 0.004 85)` | `oklch(0.22 0.006 85)` |
| `--border` | `oklch(0.89 0.005 85)` | `oklch(0.28 0.006 85)` |
| `--border-strong` | `oklch(0.82 0.006 85)` | `oklch(0.36 0.008 85)` |
| `--fg` | `oklch(0.19 0.01 260)` | `oklch(0.93 0.008 260)` |
| `--fg-muted` | `oklch(0.48 0.015 260)` | `oklch(0.78 0.010 260)` |
| `--fg-subtle` | `oklch(0.54 0.01 260)` | `oklch(0.60 0.010 260)` |
| `--sage` / `-ink` / `-bg` | `0.72 0.10 145` / `0.34 0.09 145` / `0.95 0.03 145` | `0.66 0.085 145` / `0.85 0.09 145` / `0.24 0.04 145` |
| `--amber` / `-ink` / `-bg` | `0.75 0.14 70` / `0.40 0.12 70` / `0.96 0.04 70` | `0.72 0.115 70` / `0.85 0.12 70` / `0.26 0.05 70` |
| `--terracotta` / `-ink` / `-bg` | `0.65 0.18 25` / `0.48 0.16 25` / `0.96 0.03 25` | `0.68 0.14 25` / `0.80 0.12 25` / `0.26 0.06 25` |
| `--slate` / `-ink` / `-bg` | `0.55 0.04 240` / `0.42 0.04 240` / `0.95 0.01 240` | `0.72 0.035 240` / `0.82 0.03 240` / `0.24 0.015 240` |
| `--violet` / `-bg` | `0.55 0.09 290` / `0.95 0.02 290` | `0.70 0.085 290` / `0.24 0.03 290` |
| `--p1..p4` | `#2a75ba #d06091 #654199 #00917d` | `#4790d8 #d36a96 #8962c5 #00a38f` |
| `--project-l`, `--project-c` | 0.72, 0.11 | 0.62, 0.10 |

- Other tokens:
  - Radii: `--radius-sm/md/lg` = 4, 8 and 12px.
  - Shadows: `--ring`, `--elevation-1`, `--elevation-modal`, `--scrim`. The elevation tokens are `none` in dark.
  - The `--p1..p4` colours (blue, pink, purple, teal) are for estimate progress and are not used in stats charts.
- Only `--violet` (and `--violet-bg`) has no `-ink` variant.
- Project hue: day-strip fills take a per-segment `--h` and use `oklch(var(--project-l) var(--project-c) var(--h))`, which is the in-system way to get N distinguishable categorical colours that flip correctly in dark mode.
- Fonts:
  - `--font-sans: var(--font-geist-sans), ui-sans-serif, system-ui, sans-serif`
  - `--font-mono: var(--font-geist-mono), ui-monospace, "SF Mono", Menlo, monospace`
  - Geist comes from the `geist` package, loaded in `app/layout.tsx`.
- Voice:
  - Numbers, labels, axis text and tip titles are mono with tabular-nums. `.art-label` is mono 10px, fill `--fg-muted`.
  - `.sr-title` is 11px small-caps at 0.08em.
  - Prose is sans, 12.5-15px.
  - Hero numbers are mono 40px at -0.03em.
- Card chrome: `.sr-card` is `--bg-raised`, 1px `--border`, radius 10px, padding `16px 18px`. The `sx-*-card` and `tt-card` variants use radius 12px and padding 16px. Always add `min-width: 0`.
- Dark-mode idiom for derived colours: use `color-mix(in oklch, var(--sage), black 45%)` and redeclare it under both dark selectors (`city.css:3-6`). Isometric shading comes from `tones()`.
- Hatching (not a colour) marks "none/unsynced": `--art-hatch-amber` and `--art-hatch-muted` (`app/art.css:13-14`).
- All CSS is token-only. Hex values appear only in `--p1..p4`.

## 7. Animation conventions (`app/art.css`, imported globally in `app/layout.tsx`)
- Easing is `--art-ease: cubic-bezier(0.25,1,0.5,1)` (ease-out-quart). Stroke width is `--art-stroke: 1.6px`.
- Utility classes:
  - `.art-draw` (900ms): stroke draw-in, needs `pathLength={1}`.
  - `.art-grow` (700ms): `scaleX` from 0.
  - `.art-grow-y` (700ms): `scaleY` from 0, `transform-box: fill-box`.
  - `.art-fade` (500ms).
- Staggering: use `--i` with `animation-delay: calc(var(--i)*60ms)`, as in `.sr-fill` (`charts-ranked.css:13`). Segments use inline `k*90ms`. The city uses a custom `sx-city-rise` (800ms).
- The resting state is the finished state: animations only run `from` the hidden state with `both` fill.
- `prefers-reduced-motion` is handled globally in `globals.css:224-229` (animation and transition durations go to 0.01ms), so no per-component query is needed. Only infinite loops get their own query, e.g. `.stats-flame` (`stats.css:73,103-105`).
- Hover is a stroke or filter change: `stroke-width 14 to 17` on donut segments, `filter: brightness(1.12-1.25)` on city marks, a `--fg` outline on terrain faces, and a `--bg-sunk` halo on rows.

## 8. CSS and file conventions
- One CSS file per graphic in `app/stats/*.css` (`charts-ranked`, `charts-time`, `city`, `terrain`, `tickets`, `garden`, `keycaps`, `podium`, `warehouse`, `robot`, `receipt`, `tip`). It is imported from the component, e.g. `import "@/app/stats/charts-ranked.css"`.
- Class prefixes: `sr-` (ranked/split), `sx-<name>-` (signature graphics), `tt-` (ticket timeline), `stats-` (page), `stip-` (tips), `art-` (shared).
- Most components are server-safe. Only `StatTip`, `TicketTimeline` and `DayFlow` are `"use client"`.
- Tip builders are exported pure functions.
- SVGs use `role="img"` with an `aria-label` summarising the data.
- Small viewBoxes such as `0 0 100 100` scale with the container. Scrolling charts use `.sr-scroll` with a `min-width`.
- Empty states are one italic `.sr-empty` line or `.stats-empty`.
- Mobile breakpoints are 480, 520 and 800px. At 480px, SVG label sizes are bumped (`terrain.css:21`).

## 9. Tests
- Runner: `bun test`. `bunfig.toml` preloads `happydom.ts`, which registers happy-dom globally and clears the URL and localStorage after each test.
- Dev deps: `@testing-library/react`, `@happy-dom/global-registrator`.
- Pattern:
  - `afterEach(cleanup)`, then `render(<X/>)` and assert on `container.textContent` or `getByText`.
  - Pure functions get unit tests: `*.tip.test.tsx` call the tip builder and assert exact `{title, sub, rows, bar, note}`. See `RankedBars.tip.test.tsx` (`rowTip`) and `CalendarHeatmap.tip.test.tsx` (`cellTip`).
  - `StatTip.test.tsx` covers the `data-stip` round trip, `placeTip` clamping and `fireEvent` against `TipLayer`.
  - Fixtures are in `time-fixtures.ts` (`day()`).
  - `TopicsSection.test.tsx` builds a full `TopicsReport` literal `base`.
  - It also uses `mock.module("@/lib/daemon")` and restores the previous module in `finally`.
- Conventions for new graphics: a pure `xTip` builder, a pure layout/maths function and one render test per empty state. Assert no "NaN", "undefined" or "null" in the text.

## 10. Chart libraries
None. `package.json` dependencies are only `geist`, `lucide-react`, `next`, `react` and `react-dom`. Every chart is hand-built SVG or CSS. New graphics should be hand-rolled SVG with no new dependency, which matches the existing style.

## Design notes for the designer
- To look native: use mono for numbers, token colours only, the `art-*` motion classes, `tipProps` on every mark, an `sr-card` or `sx-*-card` shell, and an `aria-label` on the SVG.
- Dark mode: any colour not already a token must be derived with `color-mix` from tokens and redeclared under both dark selectors (`prefers-color-scheme` and `data-theme="dark"`).
- Topic data limit: the contract is aggregate-only. Topic colours for N > 5 should use the project-hue idiom (`oklch(var(--project-l) var(--project-c) <hue>)`).