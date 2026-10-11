# Graphics brief — Topic atlas

**Project:** worklog's Statistics page ("Wrapped for the workday") for one developer reviewing what their weeks were about. Mood: precise, a little playful, an instrument panel at night — a star-chart / observatory feel for a section whose hero is a galaxy map of the Owner's work.

**Page system:**
- Dark-first, also light. Colours ONLY via CSS custom properties / `currentColor` — never hex. Tokens: `--fg` (oklch .93 .008 260 dark / .19 .01 260 light), `--fg-muted`, `--fg-subtle`, `--bg`, `--bg-raised`, `--bg-sunk`, `--border`, `--amber` (accent, oklch .72 .115 70 dark), `--sage`, `--slate`, `--terracotta`, `--violet`. Topic colours are per-topic hues `oklch(var(--project-l) var(--project-c) var(--h))`.
- Fonts: Geist Sans for prose, Geist Mono for numbers/labels (not used inside the art).
- Stroke feel: `--art-stroke` 1.6px, round caps/joins, flat geometry, no gradients except the page's own; existing art is isometric (city, terrain, keycaps) and flat instrument dials. Radius scale 4/8/12.
- Motion handled by the page (`.art-draw` with `pathLength=1`, `.art-fade`); assets should be static SVG with stroke paths that can take `pathLength="1"`.
- Directive from the Owner: "Make this look as amazing as possible", "epic and cool looking graphs".

**Assets:**

| id | kind | shows | facts it carries | UX job | size | placement |
|---|---|---|---|---|---|---|
| rhythm-always | icon | a steady pulse / continuous orbit ring | "Always on" = theme active on ≥ half the work days | makes the rhythm badge scannable next to its text label | 16×16 | Heartbeats row badge |
| rhythm-sprint | icon | a short sharp burst (comet streak) | "Sprint" = ≥3 days within one week | same | 16×16 | Heartbeats row badge |
| rhythm-one-off | icon | a single lone star point | "One-off" = one day only | same | 16×16 | Heartbeats row badge |
| mark-first-time | icon | a 4-point "new star" sparkle | first time = most novel block vs all earlier work | marks first-time blocks on the galaxy and moments cards | 14×14 | Galaxy dots, Moments cards |
| mark-oddball | icon | a dashed ring around a lone dot (rogue planet) | oddball = nothing else in range is like it | marks oddballs on galaxy + moments | 14×14 | Galaxy, Moments |
| empty-helper-down | spot | a telescope with its lens capped / observatory dome closed | topic stats need the local embedding helper (`worklog verdict`) | friendlier, clearer "turn the helper on" state | ~120×90 | Topic atlas when helper is off |
| empty-too-few | spot | a nearly empty star chart with 3 faint stars | fewer than 6 described blocks in range | explains "not enough described work yet" | ~120×90 | Topic atlas too_few_blocks |
| empty-no-map | spot | a star chart grid with a few dots and a dotted outline where a map would be | map needs ≥ 10 described blocks | galaxy empty state | ~120×90 | Galaxy panel |

**Output dir:** `.specs/023-topic-atlas/art/` (SVG files + GRAPHICS.md with id, file, size, alt text or aria-hidden, placement). They will be ported 1:1 into `web/components/stats/AtlasArt.tsx` as React components.
