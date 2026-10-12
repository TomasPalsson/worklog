# Style sheet — flat-geometric (seed f89d5508b6a6)

- Palette: page tokens only, via `currentColor` and `style="fill: var(--token)"`: --fg-subtle (base shapes), --fg-muted (secondary), --amber (accent / the "star"), --slate (night / cool), --bg-sunk (cut-outs). Max 4 per asset.
- Stroke: none — shapes only. One exception: the oddball ring is a dashed ring (stroke-dasharray), because the galaxy marks oddballs with dashed rings.
- Corners: crisp; rx 1 at 16 px, rx 2 at spot size.
- Shading: none, or one darker offset shape (same token at opacity .55).
- Texture: none. No gradients, shadows or blur.
- Objects: 3–6 primitives each (telescope = tube + lens + cap + tripod; star chart = card + grid dots + stars).
- Banned: outlines (except the oddball ring), gradients, shadows, texture, more than 5 colours, hex colours.
