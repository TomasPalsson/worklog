# Graphics style — monoline (locked to match the app's Lucide icons)
Seed: 4ba3844e49f2 (locked)
- Palette: currentColor only; the page sets it from tokens — sage-ink #2F5A33 (ok), amber-ink #6E4410 (low/over), fg #23252B (estimate marker). People (chart/bar only, never in icons): light #2a75ba you, #d06091, #654199, #00917d; dark #4790d8, #d36a96, #8962c5, #00a38f — validated with dataviz validate_palette.js.
- Stroke: 2.25px on a 24 grid (icons, so they hold at 14–16px), 1.5px on small objects (marker, hatch, pending disc); round caps and joins.
- Corners: round joins; no sharp miters.
- Shading / texture: none. One accent fill allowed: the estimate flag.
- People: never drawn; a person is an initials disc (live HTML text) — only the "pending" disc is an SVG.
- Banned: gradients, shadows, blur, a second stroke weight within one asset, emoji, hard-coded colours.
