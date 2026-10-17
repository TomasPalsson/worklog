# Graphics

Style: extend mode — copies the app's lucide-react line icons (24×24, stroke 2, round caps/joins, no fill, `currentColor`).

| Asset | File | Size | Alt | Placement |
|---|---|---|---|---|
| rewrite-spark — a pencil writing a line, with Claude's 4-point spark | `rewrite-spark.svg`, `RewriteSpark.tsx` | 14px in the button (24 grid) | `aria-hidden` — the button's visible "Regenerate" label names it | `InstructionBox` submit button, left of the label |

Critic: round 1 mean 6.6 (`+` read as "add", stroke too light) → spark and stroke 2; round 2 mean 7.0 → spark inset, pencil ferrule added.

open:
- At 14px the spark is a few pixels and the meaning leans on the "Regenerate" label.
- Pencil-plus-sparkle is a common "AI edit" metaphor; nothing worklog-specific in it.
