# Graphics — estimate bar on day-page blocks
Style: monoline (locked to the app's Lucide icons), seed 4ba3844e49f2 · sheet: ../../.vary/graphics-style.md
Critic round 1: mean 6.0 → fixes applied (outlined flag + halo, unbroken "over" baseline, hourglass for running low, 2.25 stroke, longer dashes on pending disc, hatch round caps).

| asset | file | size | alt / aria | placement |
|---|---|---|---|---|
| estimate flag | est-marker.svg (#est-marker) | 12×20 (drawn 13×32 on bar) | aria-hidden; the bar's aria-valuetext says the estimate | on the bar at estimate %, "4h" label beside |
| on track | state-on-track.svg | 16px | aria-hidden; text "4h left" beside | before the state words |
| running low | state-running-low.svg (hourglass) | 16px | aria-hidden; text "30m left" beside | before the state words |
| over | state-over.svg | 16px | aria-hidden; text "1h 30m over" beside | before the state words |
| this block, not in Tempo yet | person-pending.svg | 18px | aria-hidden; text "This block +1h" beside | legend |
| pending hatch | pending-hatch.svg (inlined as CSS repeating-linear-gradient in the mock) | 6px tile | decoration | bar segment for unsynced hours |
| burn-up chart | generated inline SVG per block | 260×104 viewBox, max 300px | role="img" + aria-label with daily totals; "Show as table" | right of the bar (stacks below on phone) |

open: critic round 2 not run — the evaluator pass on the whole mock replaces it.
