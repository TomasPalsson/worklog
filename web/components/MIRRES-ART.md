# Mirres art

Source: `mirres-art.tsx`. Monoline, 24 grid, 1.6 stroke, one `currentColor` wash. Colour comes from the parent's `color` (sage = billable, amber = attention, muted = not billable). No text in the art.

| Export | Sizes | aria | Placement |
|---|---|---|---|
| `BillableIcon` | 14 | `aria-hidden`; the chip/control carries the label ("Billable") | Day-page ticket row status chip; Mirres page project rows + legend |
| `IncludedIcon` | 14 | `aria-hidden`; label "Included in contract" on the control | Same as above |
| `FixedPriceIcon` | 14 | `aria-hidden`; label "Fixed price" on the control | Same as above |
| `InternalIcon` | 14 | `aria-hidden`; label "Internal" on the control | Same as above |
| `ContractMissingIcon` | 16-20 | `aria-hidden`; label "Contract missing in Mirres" on the control (pair with amber and text, never colour alone) | Day-page row (replaces the long warning), "Fix in Mirres" cards |
| `InfoIcon` | 16 | `aria-hidden`; the button carries `aria-label` ("Details") | Day-page row info button (opens the details popover) |

## Review
Style: monoline (locked to the house icons in icons.tsx). Critic rounds: 6.6 → 7.0 (stopped after 2 rounds); BillableIcon redrawn as two stacked coins after round 2.
- open: ContractMissingIcon shares BillingIcon's zig-zag foot; consider a blank signature line so it reads "contract", not "receipt".
- open: AllClearSpot could share LedgerEmpty's ledger silhouette more closely.
- open: LedgerEmpty rows hold small tag outlines; plain blank rows may read as "empty" faster.
