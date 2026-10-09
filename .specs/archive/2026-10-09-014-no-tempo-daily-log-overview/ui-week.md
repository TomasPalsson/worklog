# UI — Week page (/week/[monday]) redesign

The Owner called this page "awful". A live 1440 px screenshot showed:
- A 980 px column.
- A close-out table with raw ISO dates ("2026-09-28") and four rows in solid amber.
- Below it, 7 skinny columns of stacked block cards. Ticket keys are truncated ("GENAI-…"), each time range wraps over two lines, and the page is 5,800 px tall.
- Personal blocks are listed as more cards.
- Today is a green wash.

**Mode:** extend. Reuse the Logged v2 language: `ui-v2.md`, `web/app/logged/logged.css`, `LoggedMeter`, `web/components/icons.tsx`, the stats header and the icon toolbar.
- Existing tokens only.
- Spacing only on 4/8/12/16/24/32/48.
- Geist with Geist Mono numerals, `tabular-nums`.

**Behaviour stays.**
- Same data and the same server page fetch.
- Pull from Tempo / Sync week keep their logic: `WeekCloseout`'s `pull`, `syncWeek`, the 4 s confirm, refresh after.
- Clicking a block or a day header goes to `/<day>`.
- Ignored blocks stay hidden, as now.
- `WeekCloseout.test.tsx` behaviour assertions stay true. Update markup-only expectations.

## Page

- Wrap the page in `<div className="week-page">`.
- Add `.page:has(.week-page) { max-width: 1200px; width: 100%; }`.
- Put all styles in a NEW `web/app/week/week.css`, imported from a NEW `web/app/week/layout.tsx`.
- Delete the old week grid and close-out CSS from `globals.css` once nothing uses it. Check with grep.

## Header (`WeekHeader`)

- **Row 1:**
  - `h1` "Week of 28 Sep – 4 Oct": day-first, "Sep" not "Sept". Reuse the `weekTitle` helper from `LoggedEntries.tsx`.
  - Right-aligned toolbar:
    - `PrevIcon` icon button ("previous week").
    - "This week" text link. When current, it is plain text with `aria-current`.
    - `NextIcon` icon button ("next week").
    - The existing `DateJumper`, kept but styled as a 40 px icon button.
    - Use the same classes and looks as the Logged toolbar (`.logged-tools`, `.logged-tool`), or shared equivalents.
- **Row 2** (stats, as in Logged):
  - Worked: the work hours.
  - In Tempo: the sum of `tempo_seconds`. Show the value as "12h / 39.3h", with the target in `--fg-muted`.
  - No ticket: the sum of `unticketed_seconds`, in `--amber-ink` when it is above 0.
  - Personal: personal hours, in `--fg-muted`.
  - On the right: two buttons from the close-out.
    - `.action-btn` "Pull from Tempo" with `RefreshIcon`.
    - "Sync week", the primary button: `--fg` background, `--bg` text, `radius-md`. In its confirm state it reads "Confirm sync week" with an amber border.
    - Busy and error states as before. The error is an amber `role="alert"` box under the header.
  - The close-out component becomes the header's action slot plus the strip below. Keep `WeekCloseout` as the one client component that owns the state. It can render both the actions and the strip; the stats can live in the server header.

## Close-out strip (replaces the table)

- Lives in `WeekCloseout`.
- `display: grid; grid-template-columns: 56px repeat(7, minmax(0, 1fr)); column-gap: 8px`. The first 56 px column is empty, so the 7 days line up exactly with the timeline columns below.
- **Each day** `.week-day-card` (`data-testid="closeout-<day>"`) is a link to `/<day>`:
  - `padding: 12px; border-radius: var(--radius-md); background: var(--bg-raised); border: 1px solid var(--border)`.
  - Top row: weekday (12 px, uppercase, tracked, `--fg-subtle`) and the date "28 Sep" (14 px, 600).
  - Worked hours: mono 20 px.
  - `LoggedMeter`: in-Tempo ÷ required, row, 6 px. State colour: amber when Gap, sage when in-Tempo ≥ required, `--fg-subtle` otherwise. No meter when required is null or 0.
  - Then "1h / 7.8h in Tempo" (12 px muted). When required is null: "not pulled".
  - Chips, 11–12 px, pill radius, only when relevant:
    - "Gap": `--amber-bg` / `--amber-ink`.
    - "N pending": `--slate-bg` / `--slate-ink`.
    - "Xh no ticket": amber text, no fill.
    - "✓ done": sage, when there is no gap and nothing pending.
  - Today: a `--fg` 2 px top border. No green wash.
  - Weekend or off days with nothing: `background: transparent`.
- `margin-bottom: 24px`. The ISO dates and solid amber rows are gone.

## Timeline (replaces `WeekGrid`'s card lists)

A time-axis week calendar. Each block is placed at its real start time, with a height proportional to its length.

- **Range:** start = floor(hour of the earliest *work* block start) − 0; end = ceil(hour of the latest *work* block end).
  - Clamp so the range covers at least 08:00–18:00.
  - With no work blocks, use 08:00–18:00.
  - Personal blocks outside the range are clipped to the range edges.
- **Hour height:** `clamp(28px, 640px / hours, 56px)`. Set it as a CSS variable `--hour` on the grid.
- Use the same local-time conversion the existing `formatRange` uses (read `lib/format.ts`), so a block's position matches its printed time.
- **Grid:**
  - `grid-template-columns: 56px repeat(7, minmax(0, 1fr)); column-gap: 8px`.
  - Gutter: hour labels "09:00" (mono 11 px, `--fg-subtle`, right-aligned, `translateY(-50%)` on each hour line).
  - Each day column: `position: relative; height: calc(var(--hour) * hours)`, with faint hour lines (`repeating-linear-gradient` in `--border` at 1 px every `var(--hour)`), `border-radius: var(--radius-md)` and a `--bg-raised`-ish background at low contrast. Today's column gets a slightly stronger background.
- **Block** `.week-block`: an `<a href="/<day>">`, absolutely positioned.
  - `top: minutesFromStart / 60 * var(--hour)`.
  - `height: max(duration / 60 * var(--hour), 6px)`.
  - `left` / `width` come from lane assignment (below).
  - Look: `border-radius: 6px; padding: 4px 8px; background: var(--bg-raised); border: 1px solid var(--border); border-left: 3px solid <status>; overflow: hidden`.
  - Status colour, keeping the old meaning:
    - synced and not dirty: `--sage`;
    - synced and dirty (edited since sync): `--slate`;
    - has ticket, not synced: `--fg-muted`;
    - no ticket: `--amber`, plus an amber-tinted background (`--amber-bg`).
  - **Content by height:**
    - 32 px or more: the ticket (mono 12 px, 600) and the duration (mono 11 px, muted) on one row, and the description on a second row (12 px, ellipsis, only if there is room).
    - 18–31 px: one row with ticket and duration.
    - Under 18 px: no text.
    - Ticket text is the `jira_issue`, or "No ticket" for unassigned.
  - Always give it a `title` and `aria-label` "09:39–10:40 · GENAI-1906 · 1h 1m · <description>".
  - Hover: `--border-strong` border and a raised z-index.
- **Personal blocks:** behind the work blocks, with no border-left status and no text. Use a hatched faint fill: `repeating-linear-gradient(135deg, color-mix(in oklch, var(--fg) 8%, transparent) 0 4px, transparent 4px 8px)`. Keep the same `title`. They do not get lanes; they span the full column width, under the work blocks.
- **Overlaps:** greedy lane assignment per day for *work* blocks. Sort by start; reuse the first lane whose last end ≤ start. Width = 1 / lanes in that overlap cluster.
- **Day header:** the strip above already names the days, so the timeline has no separate header.
- **Legend** (above the timeline, right-aligned, 12 px muted, inline swatches): Synced (sage) · Ticket, not synced (muted) · No ticket (amber) · Edited since sync (slate) · Personal (hatch).

## Phone (≤ 640 px)

- Hide the timeline and legend.
- The strip becomes a vertical list of the 7 day cards, full width, each a 64 px row.
- Stats wrap 2 × 2.
- Actions are full-width buttons.

## States

- Busy: "Pulling…" / "Syncing…".
- Confirm state for Sync week.
- Error box.
- Required not pulled.
- An empty day: an empty column, and the strip shows "0h".
- An all-personal day.
- Today.
- A week with no blocks: the timeline shows 08:00–18:00 empty, with a centred muted "Nothing tracked this week."
