# Prep — Multi-tenant infra folders
Gathered: 2026-09-25 · Questions: 9 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 A block with clues for several customers is split between them, not given whole to one. APRÓ is always lowest priority: if any other customer shows up in a block alongside APRÓ, APRÓ gets 0% and the other customer gets 100%. — user, Q1
- D-02 Between two or more non-APRÓ customers in one block, hours split by clock time: each customer gets the minutes around their own clues (not by clue count, not evenly). The split must be adjustable (see D-03). — user, Q2
- D-03 "Adjustable" = per block, by hand, in the review UI (e.g. change 70/30 to 50/50). A hand-set split survives re-estimation, like `estimated_by = 'manual'` blocks. No global split-method setting. — user, Q3
- D-04 Tenant → customer mapping: match the tenant name against existing customer names/aliases automatically; any tenant that doesn't match is listed in the Billing panel for the user to link once. Unmatched tenants are never guessed. — user, Q4
- D-05 A folder is multi-tenant only when ticked once in the Billing panel. The DB is seeded with the tenant locations: `vitinn-infra/tenants/<t>` and every `genai-infra/terraform/workspaces/<ws>/<t>` (agentcore, chat-bubble, cube, knowledge-base, lakehouse, widget-artifacts, …). — user, Q5
- D-06 Work outside any tenant dir can still belong to a tenant (e.g. an MCP server that will be deployed into the sjukra tenant later), so edited paths alone are not enough: non-path clues must count too. — user, Q6
- D-07 Clue-less minutes in a multi-tenant folder go to the nearest customer clue in time (before or after). A block with zero clues falls back to the folder's pinned customer (APRÓ). — user, Q7
## Not this
- No guessing `Verkefni` — it still comes only from an explicit folder pin. — user, Q8
- No change to Tempo sync. — user, Q8
- No change to browser/Slack event routing. — user, Q8
- No fixing blocks filed under the wrong folder (e.g. block 4099 "Release worklog 0.12.0" under vitinn-infra) — separate problem. — user, Q8
## Discretion
- Exact clue set and weights (user delegated, Q6). Prep's pick: edited tenant paths > branch name > block summary/Jira text, all three count; a stronger clue beats a weaker one in the same minute.
## Assumptions
- A-01 `genai-infra` and `vitinn-infra` are pinned to APRÓ in `billing_folder_map`, and a pin wins outright, so every block in them bills APRÓ — evidence: rust/crates/worklog-core/src/billing_registry.rs:181 + billing_folder_map rows — confidence: high — unconfirmed
- A-02 A folder with no customer pin already falls back to a customer-alias match on the block's description + Jira summary — evidence: rust/crates/worklog-core/src/billing_registry.rs:183, billing.rs:562 — confidence: high — unconfirmed
- A-03 Tenants live as dirs: `vitinn-infra/tenants/<t>` (21, e.g. apro, apro-prod, sjukra, mms) and `genai-infra/terraform/workspaces/agentcore/<t>` — evidence: ls of both trees — confidence: high — unconfirmed
- A-04 `claude_work` events carry the branch and edited paths in `details` (e.g. `branch feat/sjukra-claude-3p-enable · edited …/tenants/apro-prod/…`); shell/claude_turn rows carry no tenant signal — evidence: events table, 2026-09-24 — confidence: high — unconfirmed
- A-05 2026-09-24: blocks 4098, 4101, 4102 (~2.5h) describe Sjúkra work in vitinn-infra but bill APRÓ via the pin — evidence: blocks table, day 2026-09-24 — confidence: high — unconfirmed
- A-06 Signals can disagree: a `sjukra` branch edited `tenants/apro-prod/…` files at 16:14 — evidence: claude_work 2026-09-24T16:14 — confidence: medium — unconfirmed
- A-07 A billing line's hours are the union of whole blocks; nothing splits one block across customers today — evidence: CLAUDE.md billing rules, billing.rs — confidence: medium — unconfirmed
- A-08 Some workspace subdirs are not tenants (`builds`, `uat`, `google-drive`, `tomas`); under D-04 they surface as unmatched for the user to link or dismiss — evidence: ls genai-infra/terraform/workspaces/* — confidence: medium — unconfirmed
- A-09 Seeding the registry through a migration fits the existing pattern (`SEED_CUSTOMERS`) and does not break the "edited only through the review UI" rule — evidence: rust/crates/worklog-core/src/billing_registry.rs:375, CLAUDE.md — confidence: medium — unconfirmed
## Verify
- Re-run 2026-09-24 and open the billing export: a Sjúkra line covering blocks 4098, 4101, 4102 (~2.5h) and an APRÓ line with the rest of the vitinn-infra time; then change one block's split in the review UI, re-estimate the day, and the hand-set split is still there. — user, Q9
## Open
- Q: how the split is stored and shown per block (e.g. allocation rows vs. child blocks) → deferred to spec
