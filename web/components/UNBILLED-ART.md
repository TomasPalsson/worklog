# Unbilled page art

Style: monoline, locked to the existing worklog hand (`icons.tsx`, `mirres-spots.tsx`): 24 grid, 1.6 stroke (spots 2, non-scaling), round caps and joins, `currentColor`, one wash at 0.24, no text, no ids. Colour comes from the page tokens (`--sage-ink` done, `--amber-ink` needs you, `--slate-ink` waiting, `--terracotta-ink` error).

| Asset | File | Size | Alt | Placement |
|---|---|---|---|---|
| UnbilledIcon | unbilled-icons.tsx | 22px (rail) | aria-hidden; the link has the visible label "Unbilled" | app rail |
| StepLoggedIcon | unbilled-icons.tsx | 28px | aria-hidden; label "Logged" beside it | steps row |
| StepTempoIcon | unbilled-icons.tsx | 28px | aria-hidden; label "Sent to Tempo" | steps row |
| StepBilledIcon | unbilled-icons.tsx | 28px | aria-hidden; label "Marked billed" | steps row |
| AllSettledSpot | unbilled-spots.tsx | 160×120 | aria-hidden; heading "Nothing left to finish" carries the meaning | empty state |
| DaemonDownSpot | unbilled-spots.tsx | 96×72 | aria-hidden; heading "Couldn’t reach the worklog daemon" | error state |

Critic rounds: 2 (means 7.0, then 6.8; the bar is 7.5, stop after 2 rounds).

open: UnbilledIcon still reads close to BillingIcon (both lined sheets) at 22px in the rail; give it a distinct cue (dashed outline or an open badge) in a later pass.
open: StepLoggedIcon and StepTempoIcon are vague at 16px (fine at the 28px they are used at).
open: AllSettledSpot and DaemonDownSpot read as fairly generic "settled" / "disconnected" pictures; a billing-cycle detail would make them this app's own.
- The three state icons (done / needs / waiting) were cut after critic round 2: rows now carry the strip's own swatch, so the icons had no job left.
