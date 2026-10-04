# UI — 014 Logged + shared menu

Mode: **extend**. The existing system in `web/app/globals.css` (warm paper
`--bg`, Geist + Geist Mono, sage = done, amber = attention, slate = neutral,
`--radius-md` 8px, `.day-header` / `.day-nav-btn` / `.action-btn` /
`.empty-state` / `.source-badge`) is the design. No new colour tokens, fonts
or radii; sizes below are plain px on the existing 4/8 grid.

Signature: a **state rail** — a 3px left edge on every logged day (month
cell, week row, day summary) whose colour is the `DayState`. The same rail at
three sizes is how the eye finds short days. The rail is never the only cue:
every state also has its word (§4 table).

Primary task: "which past days are short?" Focal element: an `under` day —
amber rail **and** amber tint **and** the word "Short" in 13px 600. One
label for that state everywhere: **"Short"** (cell flag, week head, day
summary, header count); the question "Is this day filled out?" appears only
in the dismiss callout.

Touch targets: every control is ≥ 44px tall at `max-width: 640px`
(`.day-nav-btn`, `.logged-views a`, `.action-btn`, `.link-btn`,
`.app-nav-link`, the dismiss input) — one rule:

```css
@media (max-width: 640px) {
  .logged-page .day-nav-btn, .logged-views a, .logged-page .action-btn,
  .logged-page .link-btn { min-height: 44px; }
  .logged-page .day-nav-btn { min-width: 44px; }
}
```

Transitions on every new interactive element: `background, color,
border-color 120ms cubic-bezier(0.25, 1, 0.5, 1)` (same as `.day-nav-btn`).
Active (pressed): `background: var(--bg-sunk)`.

## 1. Shared menu — `AppNav` (`.app-nav`)

Mounted once in `layout.tsx` inside `<main className="page">`, above
`{children}`.

```
worklog │ Day  Week  Tasks  Logged  Settings  Billing │            [◐]
──────────────────────────────────────────────────────────────────────
```

- `<nav className="app-nav" aria-label="Main">`: flex row,
  `align-items: center`, `gap: 8px`, `margin-bottom: 24px`,
  `border-bottom: 1px solid var(--border)`.
- Wordmark `.app-nav-mark`: "worklog", mono 13px 500 `--fg-muted`,
  `margin-right: 8px`. Not a link. Hidden at ≤ 640px.
- `.app-nav-links`: the six items, `display: flex; gap: 2px;
  flex: 1; min-width: 0; overflow-x: auto; scrollbar-width: none`. Only
  this row scrolls, never the page.
- `ThemeToggle` sits **outside** `.app-nav-links`, so it is always visible.
- Link `.app-nav-link`: 14px 500, `--fg-muted`, `padding: 12px 10px`
  (44px tall), `white-space: nowrap`, and a 2px underline drawn with
  `border-bottom: 2px solid transparent; margin-bottom: -1px` (sits on the
  nav's hairline). Hover: `--fg`. Current: `color: var(--fg);
  border-bottom-color: var(--fg); aria-current="page"`. The underline is a
  border, so the global `:focus-visible` box-shadow ring adds to it and
  never hides it.
- Phone (≤ 640px): `padding: 12px 8px`, 13px. Six labels ≈ 300px + gaps,
  so they usually fit 343px; when they don't, the row scrolls and the
  current link is scrolled into view on mount
  (`el.scrollIntoView({ inline: "nearest", block: "nearest" })`).
- Current section by `usePathname()`:
  `/` or `/YYYY-MM-DD` → Day; `/week…` → Week; `/tasks…` → Tasks;
  `/logged…` → Logged; `/billing…` → Billing.
- Hrefs: Day `/`, Week `/week`, Tasks `/tasks`, Logged `/logged`,
  Billing `/billing`.
- **Settings** is not a page: it is the existing `SettingsPanel` trigger
  rendered with the `.app-nav-link` class (a `<button>`, `aria-haspopup`).
  It never shows as current.
- Page headers (`DayHeader`, `WeekHeader`, Tasks, Billing) keep only their
  own title + date controls; their Tasks / Billing / Week / theme /
  settings icons go away.

## 2. Logged header (all three views)

Wrap each Logged page in `<div className="logged-page">`. Reuse
`.day-header` / `.day-title` / `.day-nav` / `.day-nav-btn`.

```
October 2026                       [Month|Week|Day]  [<] [This month] [>]
126h logged · 168h required · 2 short
```

- `h1`: "October 2026" / "Week of 28 Sep – 4 Oct" (`formatWeekRange`) /
  "Thursday, 1 October 2026".
- `.day-total` mono line, **month and week only**:
  `<logged> logged · <required> required` and, when any day is `under`,
  ` · <n> short` in `--amber-ink`. (FR-18.) The day view has no
  `.day-total`; its summary card (§6) says it once.
- View switch `.logged-views`: three links in one bordered group
  (`border: 1px solid var(--border)`, `--radius-md`, each `padding: 0 12px`,
  height 36px / 44px on phone; current `background: var(--bg-sunk)`,
  `aria-current="page"`). Targets, from the shown view's anchor day
  (month: today if the month is current, else its 1st; week: today if in
  that week, else its Monday; day: the day):
  Month → `/logged/month/<monthOf(anchor)>`,
  Week → `/logged/week/<mondayOf(anchor)>`, Day → `/logged/day/<anchor>`.
- Prev / this / next: `.day-nav-btn` chevrons with `aria-label`
  ("previous month", "next week", …) and the middle `.day-nav-btn.today`:
  "This month" / "This week" / "Today", `aria-disabled` when already there.
  Next is **not** limited — future ranges are allowed and read as
  `pending`.
- Phone: `.day-nav` gets `flex-wrap: wrap`; `.logged-views` takes its own
  line.

## 3. Fetch strip — `LoggedFetch` (`.logged-fetch`)

One line right under the header (the header's `margin-bottom` is reduced to
16px inside `.logged-page`; no negative margins), `margin-bottom: 20px`,
13px, `--fg-muted`, flex with the button at the end. The text sits in
`<p aria-live="polite">`, so screen readers hear the fetch finish.

| situation | text | look |
|---|---|---|
| fetching | "Fetching from Tempo…" | muted; Refresh disabled, `aria-busy="true"` |
| ok | "From Tempo · updated 14:02" (`pulled_at`, local time) | muted |
| failed, has data | "Couldn't reach Tempo — showing data from 14:02." + reason | warning box (below) |
| failed, nothing stored | "Couldn't reach Tempo — nothing stored yet." + reason | warning box |

Reason sentence, from the daemon's `{error}` text: matches
`/token|401|403|unauthor/i` → "Tempo didn't accept the token — check it in
Settings."; matches `/timed? ?out|timeout/i` → "Tempo took too long — try
Refresh again."; anything else → "Try Refresh again in a minute." The raw
`{error}` text goes only in the box's `title` (never as visible copy).
Warning box: `background: var(--amber-bg); color: var(--amber-ink);
padding: 8px 12px; border-radius: var(--radius-md); role="alert"` —
amber (attention), not terracotta (destructive): nothing was lost.

Refresh: `.action-btn` "Refresh from Tempo" with the `RefreshCw` lucide
icon (14px). While fetching the icon spins (`transform: rotate`); the
global reduced-motion rule stops it.

## 4. Month view — `LoggedMonth` (`.logged-month`)

```
 MON      TUE      WED      THU      FRI      SAT      SUN
┃28      ┃29      ┃30      ┃ 1      ┃ 2      ┃ 3      ┃ 4
┃8h      ┃8h      ┃5h      ┃8h      ┃7.5h    ┃        ┃
┃of 8h   ┃of 8h   ┃of 8h   ┃of 8h   ┃of 8h   ┃off     ┃today
┃✓       ┃✓       ┃Short   ┃✓       ┃Short   ┃        ┃
```

(Any past day under required is `under` — 7.5h of 8h too. FR-09 has no
tolerance.)

- `<ol className="logged-month">`, `display: grid;
  grid-template-columns: repeat(7, 1fr); gap: 4px`. Weekday header row
  `.logged-month-head` (12px mono uppercase, `letter-spacing: 0.08em`,
  `--fg-subtle`), `aria-hidden`.
- Each `<li>` holds `<Link className="logged-cell" data-state={state}
  href=/logged/day/<day>>`: `display: flex; flex-direction: column;
  gap: 2px; min-height: 88px; padding: 8px 10px; background:
  var(--bg-raised); border: 1px solid var(--border); border-left: 3px
  solid <rail>; border-radius: var(--radius-sm)`. Hover / active
  `background: var(--bg-sunk)`.
  - `.logged-cell-num`: day of month, 13px 600.
  - `.logged-cell-hours`: mono 15px, `formatTotalHours(logged_seconds)`.
  - `.logged-cell-req`: mono 12px `--fg-muted`, "of 8h".
  - `.logged-cell-flag`: 13px, the state word (table).
  - `aria-label`: "<Weekday> <d> <Month>: <logged> of <required>, <state
    word>", e.g. "Wednesday 30 September: 5h of 8h, short";
    "Friday 9 October: not fetched yet"; "Saturday 3 October: day off";
    "Wednesday 30 September: 5h of 8h, marked fine: dentist".
- Days outside the month (`data-outside`): text colours drop to
  `--fg-subtle` (hours, num, req) and the cell background to `--bg`. No
  opacity, so AA holds. Still links.
- **By `data-state`**:

| state | rail (`border-left-color`) | flag text | extra |
|---|---|---|---|
| `full` | `--sage` | "✓" + visually-hidden "full", `--sage-ink` | |
| `under` | `--amber` | "Short" `--amber-ink` 600 | cell `background: var(--amber-bg)` |
| `dismissed` | `--slate` | the reason, 1-line ellipsis, `--slate-ink` | |
| `off` | `--border` | "off" `--fg-subtle` | hours hidden when 0 |
| `pending` | `--border-strong` | "today" on today, nothing later | today (any state): `.logged-cell-num` in a 22px `--fg` circle with `--bg` text, `aria-current="date"` |
| `not_fetched` | `--border-strong` | "not fetched" `--fg-subtle` | whole cell `border-style: dashed`; hours shown as "—", never "0h" |

  Sage and amber rails are close in lightness; the tint, the ✓ and the
  words carry the difference, so colour is never alone.
- When the post-mount fetch changes a cell, it just re-renders (no
  animation); the fetch strip's `aria-live` line announces the update.
- **Short-day list** `.logged-short` (month only, when any day is
  `under`): one line under the fetch strip, 13px: "Short: " then links to
  each `under` day's `/logged/day/<day>` ("Wed 30 Sep, Fri 2 Oct"),
  `--amber-ink`. Dismissing stays on the day/week view.
- **Phone** (≤ 640px): `.logged-month` → one column; `.logged-month-head`
  and `[data-outside]` hidden; each cell becomes a 48px row
  (`min-height: 48px; display: grid; grid-template-columns: 4.5rem 1fr
  auto; align-items: center`): "Thu 1" · "8h of 8h" · flag. The weekday
  comes from a `.logged-cell-dow` span, `display: none` on desktop.

## 5. Week view — `LoggedWeek` (`.logged-week`)

A `<ol>` of the 7 days:

```
┃ Wed 30 Sep                         5h of 8h   Short
┃   PROJ-12   2h    Fixed the export race          sent by worklog
┃   PROJ-40   3h    Review                          hand-logged
┃   Is this day filled out? 5h of 8h logged.   [Mark as fine…]
```

- `.logged-week-day[data-state]`: `border-left: 3px solid <rail>` (same
  table as §4; `under` also gets the `--amber-bg` tint on its head row),
  `padding: 12px 16px`, `border-bottom: 1px solid var(--border)`.
- Head `.logged-week-head`: `<Link>` day name (14px 600) to
  `/logged/day/<day>`, then mono `5h of 8h` and the flag word, right-aligned.
- Entries: the shared `LoggedEntries` list (§6), 13px.
- `under` → `DismissDay` (§7) under the entries; `dismissed` → its
  dismissed line.
- `not_fetched` → "Not fetched from Tempo yet — use Refresh above." muted,
  no list.

## 6. Day view — `LoggedDay` (`.logged-day`)

- Header as §2 (no `.day-total`), then summary card
  `.logged-day-sum[data-state]` with the rail: mono 28px `5h`, "of 8h
  required", the flag word.
- `DismissDay` (§7) directly under it on `under` / `dismissed`.
- Entries `<ul className="logged-entries">`, one `<li>` per entry,
  `display: grid; grid-template-columns: 7rem 4rem 1fr auto; gap: 12px;
  padding: 12px 0; border-bottom: 1px solid var(--border)`:
  - ticket: `jira_issue` in mono; when null, mono `--fg-muted`
    "Tempo #<issue_id>" (visible text, no tooltip).
  - hours: mono, `formatTotalHours(seconds)`.
  - description; empty → "No description" in `--fg-subtle` (not italic).
  - source: reuse `.source-badge` with a modifier — `.source-badge.worklog`
    "sent by worklog" (12px here, not 11; `--sage-bg` / `--sage-ink`), `.source-badge.outside`
    "hand-logged" (`--slate-bg` / `--slate-ink`). No new badge class.
- Phone: `grid-template-columns: 1fr auto`; ticket + hours row 1,
  description row 2 (span 2), badge row 3.
- Empty: `.empty-state` —
  `off` / `pending`, no entries: "Nothing logged in Tempo for this day."
  + for a past day: "Log time in Tempo, then press Refresh from Tempo."
  `not_fetched`: "Not fetched from Tempo yet." + "Press Refresh from Tempo
  above to load it."

## 7. Dismiss control — `DismissDay` (`.dismiss-day`)

Only on `under` and `dismissed` days (day + week views). Month cells link
to the day instead.

- **under, closed**: callout `background: var(--amber-bg);
  color: var(--amber-ink); padding: 10px 14px; border-radius:
  var(--radius-md)`; flex, wraps on phone: "**Is this day filled out?**
  5h of 8h logged." + `.action-btn` "Mark as fine…".
- **open**: the button becomes an inline `<form>`:
  `<label for>` "Why is this day short?" (visible), `<input>` (no
  `maxLength`, so typing is never silently stopped; `--bg-sunk`,
  `--radius-sm`, 36px / 44px phone), counter `12/80` mono 12px
  `--fg-muted` (`--terracotta-ink` past 80), `.action-btn` "Save reason"
  and `.link-btn` "Cancel". Input gets focus on open.
  - Validation on submit and on blur: trimmed empty → hint under the input
    "Write a short reason, like “dentist”."; over 80 → "Keep it to 80
    characters (now 93)." Hint is `--terracotta-ink` 12px,
    `aria-describedby` from the input, `aria-invalid="true"`. Nothing is
    sent while invalid. "Save reason" stays enabled so pressing it shows
    the hint.
  - Enter submits; Esc / Cancel closes and focuses "Mark as fine…".
- **saving**: "Save reason" disabled, label "Saving…".
- **daemon error**: same hint slot, `role="alert"`, the daemon's `{error}`
  text; input keeps its value.
- **dismissed**: slate line `background: var(--slate-bg);
  color: var(--slate-ink); padding: 10px 14px; border-radius:
  var(--radius-md)`: "Marked fine: **dentist**" + `.link-btn` "Undo".
  After Save, focus moves to "Undo"; after Undo, to "Mark as fine…".
- `.link-btn` (new, small): text button, 13px 500, `--fg-muted`,
  underline on hover, `padding: 8px`, ≥ 44px tall on phone.

## 8. Light / dark

Every colour above is an existing token defined in both theme blocks, so
dark mode needs nothing new. `not_fetched` uses `--border-strong` for its
dashed border so it shows on dark `--bg-raised`.

## 9. CSS placement

New rules go in `web/app/globals.css` under a `/* logged */` section
(month / week / day / dismiss / fetch, `.link-btn`,
`.source-badge.worklog|outside`) and an `/* app nav */` section. Existing
tokens only; no new custom properties.
