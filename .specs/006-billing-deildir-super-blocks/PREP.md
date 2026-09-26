# Prep — Billing deildir and super blocks
Gathered: 2026-09-26 · Questions: 10 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 A deild is only a name (the Verkefni text). Askrift vs hourly, and reikningshæfi, are decided by the external billing system — worklog stores no pricing type. — user, Q1
- D-02 Each customer has a list of deildir. A block's deild is guessed from per-deild keywords matched in its text first, then the folder's default deild if no keyword matches (mirrors customer aliases). — user, Q2
- D-03 A super block is a group, not a DB merge: one billing line holding every block with the same customer + deild; blocks stay separate, and a block leaves the group by changing its deild. — user, Q3
- D-04 One block can be split by % across several deildir (not limited to one deild per block or per customer part). — user, Q4
- D-05 The existing "Change" split editor grows to rows of (customer, deild, %). The same customer may appear on several rows with different deildir, e.g. Sjúkra·Rekstur 33 / Sjúkra·Áskrift 33 / APRÓ·… 34. — user, Q5
- D-06 Automatic changes notify both live (pop-up while the page is open) and as a catch-up ("N things changed since your last visit"). Every notice names its source: Claude estimator, Verdict model, keyword/alias guess, or the user. — user, Q6
- D-07 One pop-up per batch per source (e.g. "Claude changed 20 blocks · show"); opening it lists each change. Never one pop-up per block. — user, Q8
- D-08 Notified changes: a block's customer, deild or % split changing, and Claude rewriting a description. Not notified: new/deleted blocks, time/duration changes. — user, Q9
## Not this
- Never auto-change a deild, customer or split the user set by hand. — user, Q7
- Notifications stay in the app: no email, phone or Slack. — user, Q7
- No changes to Tempo or Jira. — user, Q7
- No askrift/hourly flag or pricing on a deild. — user, Q1
- Reikningshæfi (billable) logic — the time clocking system derives it from the Verkefni. — user, idea
- Jira ticket assignment — user doesn't care about it for billing. — user, earlier in session
## Discretion
- Where the deildir list is edited: the Settings → Billing customer section (registry is UI-only per CLAUDE.md).
- Migration: an existing folder pin's Verkefni becomes that folder's default deild under its pinned customer.
- Retention of the catch-up list and how "last visit" is tracked.
- Pop-up wording, placement and styling.
## Assumptions
- A-01 A "deild" is the existing `Verkefni` field on a billing line — evidence: web/lib/types.ts:259 ("Verkefni (deild)") — confidence: high — confirmed Q1
- A-02 Today a Verkefni comes only from a per-folder pin, one value per folder — evidence: rust/crates/worklog-core/src/billing_registry.rs:39, billing.rs:618 — confidence: high — unconfirmed
- A-03 Billing lines already group by (customer, verkefni, ticket-or-folder); ticket and folder split lines that share customer+verkefni — evidence: rust/crates/worklog-core/src/billing.rs:486,562 — confidence: high — unconfirmed
- A-04 A per-block customer % split already exists (block_customer_shares, "Change" editor) — evidence: rust/crates/worklog-core/sql/schema.sql:224, web/components/BlockCustomerSplit.tsx — confidence: high — unconfirmed
- A-05 Line hours must stay the union of block intervals, never a sum — evidence: CLAUDE.md "Billing export" rule — confidence: high — unconfirmed
- A-06 "The decision model" covers: the estimator rewriting description/ticket, block re-infer, customer resolution from aliases/tenant clues, and the Verdict event router — evidence: estimate.rs:860, infer.rs:547, billing.rs:421, verdict.rs:1 — confidence: medium — unconfirmed
- A-07 Pop-ups today are toasts with tones ok|error only; the only change alert is "Moved from X to Y" after the user's own description edit — evidence: web/lib/toast.ts:6, web/components/BlockCard.tsx — confidence: high — unconfirmed
## Verify
- On 2026-09-25 real data: add deildir to Sjúkra and APRÓ; the Billing view shows one line per (customer, deild); split one vitinn-infra block Sjúkra·Rekstur 50 / APRÓ·AI hraðall 50 and both lines update; re-run estimate → one batched pop-up naming Claude as the source, and the hand-set split is unchanged. — user, Q10
## Open
