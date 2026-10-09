# UI v2 — Logged, roomier (replaces ui.md §2–§7 visuals)

Why: the Owner found v1 "incredibly cramped". Live screenshots (1440 px) showed:
- every Logged page squeezed into the 980 px `.page` column;
- month cells stacking 4 lines of 12–15 px text in the top-left, 4 px gaps;
- "of 7.8h" repeated in all 31 cells;
- four stacked bars above the grid (header, totals, fetch line, "Short:" line);
- week rows with amber header tints that touch their text, and a full-width amber callout under every short day;
- "Sept" vs "Sep" mixed.

Mode: **extend**. Keep the system: Geist + Geist Mono, warm paper tokens (`--bg`, `--bg-raised`, `--bg-sunk`, `--border`, `--fg*`, sage / amber / slate), `--radius-sm|md|lg`, `.day-nav-btn`, `.action-btn`, AppNav. No new colour tokens. Use only these sizes: 4 / 8 / 12 / 16 / 24 / 32 / 48 px.

## Concept and signature

The Logged pages read like a filled-in timesheet. Each day is a **fill meter**: a bar whose filled part is `min(logged / required, 1)`.

Meter colour by `DayState`:

| state | colour |
|---|---|
| `full` | `--sage` |
| `under` | `--amber` |
| `dismissed` | `--slate` |
| `pending` (past or today) | `--fg-subtle` |
| `off`, `not_fetched` | none: track only, or no meter |

The track is `--bg-sunk` with `border-radius: 999px`. The meter appears at three sizes:

| where | size |
|---|---|
| month cell | 6 px tall, full width, at the bottom |
| week strip | 96 px tall, vertical |
| day summary | 10 px tall, full width |

The meter replaces the v1 3 px left rail everywhere. Meaning never rests on colour alone; the state word stays.

One shared component: `LoggedMeter({ logged, required, state, orientation?: "row" | "column" })`. It renders `role="meter"` with `aria-valuemin=0`, `aria-valuemax=required`, `aria-valuenow=logged` and `aria-label`. Fill width/height is set via an inline CSS variable, e.g. `style={{"--fill": "64%"}}`.

## Page width

`.page:has(.logged-page) { max-width: 1200px; }`. Only the Logged pages widen; other pages keep 980 px.

## Header (all three views): `LoggedHeader`

```
October 2026                                    [Month|Week|Day]  [<] [This month] [>]
LOGGED        TARGET          SHORT                       From Tempo · updated 15:13
9h            172.7h          1 day · Thu 1 Oct          [⟳ Refresh from Tempo]
```

- **Row 1:** `h1` at 32 px, weight 600, `letter-spacing: -0.025em`, `line-height: 1.1`. Controls on the right, as in v1.
- **Row 2** (`.logged-stats`, `margin-top: 24px`): a flex row.
  - **Left:** 3 stats, gap 48 px. Each stat is a label over a value.
    - Label: 12 px, uppercase, `letter-spacing: 0.08em`, `--fg-subtle`.
    - Value: mono 28 px, `font-variant-numeric: tabular-nums`, `line-height: 1.1`, `margin-top: 4px`.
  - **Stats:**
    - **Logged:** the sum.
    - **Target:** the sum of required.
    - **Short:** the count of `under` days. It reads "0" in `--fg-muted` when there are none.
      - When the count is > 0, the value is `--amber-ink`.
      - On month and week only, the short days are listed under the number as small links (13 px, `--amber-ink`, comma separated, e.g. "Thu 1 Oct"). They link to `/logged/day/<day>`.
  - **Day view stats:** "Logged" and "Target" only. The day summary (below) carries the meter.
  - **Right:** `LoggedFetch`, right-aligned, `align-self: flex-end`. Status text (13 px muted) sits on the line above the Refresh button, with the text right-aligned and an 8 px gap.
  - **Errors:** the error box is not squeezed in here. It renders full width under the header, 16 px below it, still the amber `.logged-fetch-warn` box.
- The header keeps its bottom border and gets `margin-bottom: 32px`. Remove v1's separate `.logged-fetch` row spacing, the `.day-total` line and the `.logged-short` line.
- `LoggedFetch` stays its own client component. Only its placement and markup change: give it a `variant="stack"` layout or just CSS. Keep its logic and copy as they are.

## Month: `LoggedMonth`

- **Weekday header row:** 12 px, uppercase, tracked 0.08em, `--fg-subtle`, `padding: 0 4px 8px`.
- **Grid:** `gap: 8px`.
- **Cell `.logged-cell`:**
  - Box: `min-height: 120px; padding: 12px 16px 16px; display: flex; flex-direction: column; border-radius: var(--radius-md); background: var(--bg-raised); border: 1px solid var(--border)`. No left rail.
  - Hover: `border-color: var(--border-strong)`, plus `--bg-sunk` background on press. Transition 120 ms, `cubic-bezier(0.25, 1, 0.5, 1)`.
  - **Top row** (flex, space-between, baseline):
    - Day number: 15 px, weight 600.
    - State word on the right: 12 px.
      - `under`: "Short", `--amber-ink`, weight 600.
      - `dismissed`: the reason in `--slate-ink`, ellipsis, max 9 ch.
      - `off`: "off", `--fg-subtle`.
      - `not_fetched`: "not fetched", `--fg-subtle`.
      - `full`: "✓" with a visually hidden "full".
      - `pending` today: a "today" pill (11 px, `--fg` text on `--bg-sunk`, radius 999, padding 2/8).
  - **Hours** (`margin-top: auto`, so they sit low): mono 22 px, `letter-spacing: -0.02em`, tabular.
    - Shown when `logged_seconds > 0`, or when the day is past and has required > 0 (that is, show "0h" on short days).
    - Not shown on future or `off` days with 0. `not_fetched` shows "—".
    - Nowhere does the cell show "of 7.8h"; the Target stat says it once. Keep it in the cell's `aria-label`.
  - **Meter:** `margin-top: 12px`, 6 px tall. Only when `required > 0` and the state is not `not_fetched`.
  - **Today:** the number sits in a 24 px circle (`--fg` background, `--bg` text). The v1 rule is kept.
  - **`under` cell:** `border-color: color-mix(in oklch, var(--amber) 55%, var(--border))`. No full amber fill; the meter and the word carry it.
  - **`off` cell and weekend cells with no entries:** `background: transparent`, border `--border`, number and text in `--fg-subtle`. They recede.
  - **`not_fetched`:** dashed border `--border-strong`.
  - **Outside the month:** `background: transparent`, everything `--fg-subtle`, no meter.
- **Phone (≤ 640 px):** the cell becomes a 56 px row:
  - `display: grid; grid-template-columns: 4.5rem 1fr 4rem auto; align-items: center; gap: 12px; padding: 0 16px`.
  - Columns: weekday + date | meter (row, 6 px) | hours (mono 15 px, right) | state word.
  - Rows are separated by 4 px.
  - Weekday header and outside-month days are hidden.

## Week: `LoggedWeek`

1. **Week strip** `.logged-week-strip` at the top, `margin-bottom: 32px`:
   - `display: grid; grid-template-columns: repeat(7, 1fr); gap: 12px`.
   - Each column is a link to `#d-<day>` (same page), `padding: 12px; border-radius: var(--radius-md)`. Hover: `--bg-raised`.
   - **Column content, top to bottom:**
     - weekday (12 px, uppercase, tracked, `--fg-subtle`);
     - date (15 px, 600);
     - a vertical meter, 96 px tall × 12 px wide, centred, `margin: 12px auto`;
     - hours (mono 15 px, tabular);
     - state word (12 px, same palette as the month).
   - The strip is the at-a-glance view of the week.
2. **Day cards** `.logged-week-day` (with `id="d-<day>"`):
   - `background: var(--bg-raised); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 20px 24px`, and 16 px between cards.
   - **Card head**, a flex row:
     - **Left:** a link to `/logged/day/<day>` with the day name (16 px, 600) and the date in `--fg-muted` ("Wednesday · 30 Sep"). The underline appears on hover only.
     - **Right:** mono "5h / 7.8h" (15 px tabular), a horizontal meter (120 px × 6 px), and the state word. For `under`, the compact `DismissDay` trigger "Mark as fine…" appears here as an `.action-btn` (36 px, 44 px on phone).
   - **Body** (`margin-top: 16px`):
     - The entries table (see Day view).
     - `under`/`pending`-past with no entries: one muted 14 px line, "Nothing logged in Tempo."
     - `not_fetched`: "Not fetched from Tempo yet — use Refresh above."
     - `dismissed`: the slate line "Marked fine: **dentist** · Undo" sits in the head under the title, 13 px, not as a big box.
   - **Collapsed rows:** `off` days and future `pending` days with no entries render as a slim row instead of a card: `padding: 12px 24px; background: transparent; border: 1px dashed var(--border)`, with the head only.
3. The v1 amber header tint and the full-width "Is this day filled out?" callout in the week are removed.

## Day: `LoggedDay`

- **Summary card** `.logged-day-sum`:
  - `padding: 24px; border-radius: var(--radius-lg); background: var(--bg-raised); border: 1px solid var(--border)`.
  - **Row 1:** big hours (mono 48 px, `letter-spacing: -0.03em`, `line-height: 1`), then "of 7.8h target" in `--fg-muted` (15 px), and the state word on the right.
  - **Row 2:** the meter (10 px, full width, `margin-top: 16px`).
  - **Row 3**, only when `under`/`dismissed`: `DismissDay`, inline (`margin-top: 16px`).
    - A single line: "Is this day filled out? 5h of 7.8h logged." in `--amber-ink`, then the "Mark as fine…" button.
    - No separate amber box.
    - The form and the dismissed states are unchanged in behaviour.
- **Entries** (`margin-top: 32px`):
  - A small heading row: "6 entries · 8h" (12 px, uppercase, tracked, `--fg-subtle`, `padding-bottom: 8px`, `border-bottom: 1px solid var(--border)`).
  - **Rows:** `display: grid; grid-template-columns: 8rem 4.5rem minmax(0, 1fr) auto; column-gap: 24px; padding: 16px 0; border-bottom: 1px solid var(--border)`.
  - **Columns:**
    - ticket: mono 14 px;
    - hours: mono 14 px, right-aligned, tabular;
    - description: 14 px, `line-height: 1.55`, `max-width: 72ch`;
    - source: the `.source-badge` pill, `justify-self: end`, `align-self: start`.
  - **Phone:** row 1 is ticket + hours, row 2 the description, row 3 the badge.
- **Empty state:** keep the v1 `.empty-state` copy, but put it inside a dashed `--border` box with `padding: 32px`.

## Shared fixes

- **Dates:** `dayLabel`/`dayNum`/`monthTitle`/`dayTitle` must give the same text on server and client, and "Sep", never "Sept".
  - Use fixed English arrays `["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"]` plus a weekday array instead of `toLocaleDateString` short months.
  - Long names come from long arrays.
  - Week title: "Week of 28 Sep – 4 Oct" in day-first order, matching the rest of Logged.
- **Focus:** keep the global `:focus-visible` ring on every link and button. Meters are not focusable.
- **Motion:** none, except the existing spinner, which already honours reduced motion.

## Navigation: icon side rail (replaces the top AppNav row)

The Owner hated both top bars (the text menu and the Logged Month/Week/Day + arrows bar) and asked for icons. The icons are custom SVGs in `web/components/icons.tsx`. Use them; never add lucide icons to the rail or the Logged header.

### Rail (`AppNav`, desktop > 640 px)

- `layout.tsx` wraps the body content in `<div class="app-shell">` with `display: grid; grid-template-columns: 76px minmax(0, 1fr)`.
- The rail is the first column: `position: sticky; top: 0; height: 100vh; border-right: 1px solid var(--border); background: var(--bg); padding: 16px 8px`. It is a flex column with centred items.
- `<main class="page">` is the second column. It keeps its own max-width and centring inside that column (1200 px for Logged, via the existing `:has` rule).
- **Rail contents, top to bottom:**
  - `LogoMark` (28 px, `--fg`), a link to `/` with `aria-label="worklog home"`, `margin-bottom: 24px`.
  - Day (`DayIcon`, `/`), Week (`WeekIcon`, `/week`), Tasks (`TasksIcon`, `/tasks`), Logged (`LoggedIcon`, `/logged`).
  - A spacer (`margin-top: auto`).
  - Billing (`BillingIcon`, `/billing`), Settings (`SettingsIcon`, the existing `SettingsPanel` trigger, a button) and the theme toggle.
- **Item `.app-rail-item`:** 60 px wide, at least 56 px tall, a flex column, centred, `border-radius: var(--radius-md)`.
  - Icon 22 px, with a **visible label** under it: 11 px, weight 500, `margin-top: 4px`. Labels stay for scanning.
  - Colour `--fg-muted`; hover `--fg` on `--bg-raised`.
  - **Current item:** `--fg` on `--bg-raised`, `aria-current="page"`, and a 3 × 20 px `--fg` pill at the rail's left edge (`::before`, absolutely positioned, `left: -8px`).
  - Transition: 120 ms `cubic-bezier(0.25, 1, 0.5, 1)` on colour and background only.
- **Theme toggle:** the same item shape, icon only (no label), with `aria-label` and `data-tip`. The icon follows the state: system → `SystemThemeIcon`, light → `SunIcon`, dark → `MoonIcon`. Restyle `ThemeToggle` to use these icons and an optional `className` prop; its behaviour does not change.
- **Tooltips:** in the rail, `[data-tip]` tooltips open to the right: `.app-rail [data-tip]::after { left: calc(100% + 10px); top: 50%; translate: 0 -50%; }` and the matching hover/focus rule.
- **Unchanged from v1 (FR-14):** the current-section mapping, the hrefs, exactly one `aria-current="page"`, Settings never marked current, Settings as a button with `aria-haspopup`, and `<nav aria-label="Main">`.

### Phone (≤ 640 px)

The rail becomes a bottom tab bar:

- `position: fixed; left: 0; right: 0; bottom: 0; height: calc(64px + env(safe-area-inset-bottom)); padding-bottom: env(safe-area-inset-bottom)`.
- A flex row with `justify-content: space-around`, `border-top: 1px solid var(--border)`, `background: var(--bg)`, `z-index: 40`.
- Items: 44 × 52 px, 10 px labels. The logo and the spacer are hidden. The current item's pill moves to the top edge.
- `.app-shell` becomes one column, and `.page` gets `padding-bottom: 104px`.

### Logged header toolbar (replaces the v1 Month|Week|Day + ‹ This › bar)

- `.logged-tools`, right-aligned on the `h1` row:
  - **View toggles:** three icon buttons (`MonthViewIcon`, `WeekViewIcon`, `DayViewIcon`).
    - 36 × 36 px (44 on phone), `border-radius: var(--radius-md)`, `--fg-muted`, hover `--fg`.
    - The current one is `--fg` on `--bg-raised` with a `--border` border and `aria-current="page"`.
    - Each has `aria-label` "Month view" / "Week view" / "Day view" and a matching `data-tip`.
    - 4 px gaps, no group border.
  - Then a 16 px gap.
  - Then `PrevIcon` button, a small text link, and `NextIcon` button.
    - Icon buttons: `aria-label` "previous month" / "next month" etc.
    - Text link: "This month" / "This week" / "Today", 13 px, `--fg-muted`, underline on hover. When already on it, render it as plain `--fg-subtle` text with `aria-current`.
- The Refresh button uses `RefreshIcon` (16 px).

## States checklist (all must render)

- Fetching: status text plus a spinning icon on Refresh.
- Fetch failed with data, and fetch failed with nothing stored: the amber box under the header.
- `not_fetched` cells and cards.
- An empty past day.
- Today.
- Future days.
- Off days.
- Dismissed: the reason in the cell and in the head, with Undo.
- Dismiss form: validation hint, saving, daemon error.
