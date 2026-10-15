# Graphics brief — estimate progress bar on day-page blocks

**Project:** worklog review UI (internal tool, one developer + teammates at APRÓ). The day page lists time blocks; each block now shows how much of its Jira ticket's estimate is used, split per person. Mood: calm, precise, "quiet editorial broadsheet" — a ledger, not a dashboard. The user glances at it many times a day; it must read in under a second.

**Page system (extend mode — match exactly, do not invent a new style):**
- Source of truth: /Users/tomas/Desktop/Projects/worklog/web/app/globals.css (tokens at :root and dark blocks).
- Light: bg #FBFAF7 (oklch .985 .003 85), raised #FEFEFC, sunk #F5F3EF, border #E0DED9, fg #23252B (oklch .19 .01 260), fg-muted #646A75, sage #7FBF85 / sage-ink #2F5A33 / sage-bg #E6F2E5, amber #E7A73A / amber-ink #6E4410 / amber-bg #FCF0DC, slate #6E7F8F / slate-ink #4F5E6C.
- Dark: bg oklch .15 .006 85, accents desaturated (sage-ink oklch .85 .09 145, amber-ink oklch .85 .12 70).
- Fonts: Geist (sans), Geist Mono (numbers, keys).
- Radius: 4 / 8 / 12px. Icons in the app are Lucide: 24-grid, 1.5–2px round stroke, no fill.
- Bans counted from existing UI: no gradients, no drop shadows, no blur, no emoji, no photo-real anything. Colour must come from `currentColor` or CSS vars so dark mode works.
- User directive: "I want a progress bar … make it look all pretty."

**Assets:**

| id | kind | what it shows | facts it carries | UX job | size | placement |
|---|---|---|---|---|---|---|
| est-marker | object | A small flag/pin marker that stands on the progress bar at the estimate point, with a stem down through the bar | the Jira "Estimated" value (e.g. 4h) | shows exactly where the allowed hours end, so "over" is visible as fill past the flag | 12×20 px, SVG, currentColor | absolutely positioned on the bar at estimate %, label "4h" beside it |
| state-icons | icon-set | 3 tiny status glyphs: on track (check in circle), running low (half-full hourglass or gauge), over (arrow up past a line) | ticket state vs estimate | colour is never the only signal; glyph + words | 14×14 px each, Lucide-compatible stroke, currentColor | before the "4h left" / "30m left" / "1h 30m over" text on each block |
| person-dot | icon-set | Initials disc for a person (filled circle with 1–2 letters) plus a "this block, not in Tempo yet" variant (dashed outline disc with a small plus) | who logged the hours: You, Jón Geir; this block's pending hours | ties each bar segment to a person in the legend | 16×16 px, SVG template with fill via CSS var | legend under the bar |
| pending-hatch | pattern | Diagonal hatch tile for the bar segment of hours not yet in Tempo | this block's unsynced 1h | separates "already in Tempo" from "will be after sync" | 6×6 px repeating tile, SVG, currentColor | background of the pending bar segment |

**Output dir:** /Users/tomas/Desktop/Projects/worklog/.specs/020-tempo-estimate-per-person/assets/art/
Write GRAPHICS.md there with each file, size, alt text or aria-hidden, and placement.
