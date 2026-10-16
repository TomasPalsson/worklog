# Prep — Right-click menu on blocks, with regenerate-from-my-prompt
Gathered: 2026-10-16 · Questions: 7 of 12 · Route: dispatch · Status: ready for spec

## Decisions
- D-01 "Regenerate with my prompt" rewrites ONLY the block's description. Duration, Jira ticket and estimated_by stay as they are. Motive: fix bad AI descriptions. — user, Q1
- D-02 Right-click menu mirrors today's per-block actions: Describe with Claude, Personal/Work toggle, Ignore → Noise / Duplicate / Internal, Delete. The existing icon buttons stay. — user, Q2
- D-03 The user's prompt is one-time: sent with this one regenerate for this one block, never stored or applied to later descriptions. — user, Q5
## Not this
- Prompt-regenerate never changes duration or ticket. — user, Q1
- No menu items for things already one click away on the card (edit/copy description, ticket picker, details link, undo). — user, Q3
- Day page only: no right-click menu in the billing view or Tempo/Mirres pages. — user, Q6
- No multi-select / acting on many blocks at once. — user, Q6
- No touch long-press support. — user, Q6
## Discretion
- Where the prompt is typed (inline box in the menu vs small popup) — interaction feel, ungrillable; spec/build decides.
- Extra menu actions beyond D-02 — user delegated ("you choose, just not ones I can already do"), Q4. Mr Claude's pick: Split block in two (`/blocks/:id/split`, no UI today) and Merge with previous / next block (`/blocks/merge`). Spec may adjust, but never adds an item already one click away on the card.
## Assumptions
- A-01 Block cards render in `BlockCard`; per-block actions today are icon buttons (Sparkles describe, Coffee personal, IgnoreMenu) — evidence: web/components/BlockCard.tsx:315-352 — confidence: high — unconfirmed
- A-02 No `onContextMenu` exists anywhere in web/app or web/components today — evidence: grep, no hits — confidence: high — unconfirmed
- A-03 "Regenerate" already exists as Sparkles → `describeBlock` → daemon `POST /blocks/:id/estimate`, which re-runs Claude on events + commits and overwrites description / duration / ticket, even manual ones — evidence: web/app/actions.ts:445, rust/crates/worklog-core/src/daemon.rs:206,1726 — confidence: high — unconfirmed
- A-04 That endpoint takes no request body today, so there is no way to pass a user prompt yet — evidence: rust/crates/worklog-core/src/daemon.rs:1732-1735 — confidence: high — unconfirmed
- A-05 A dropdown menu pattern already exists to reuse (`ActionsMenu` + `menuKeys` + `useMenuDismiss`) — evidence: web/components/ActionsMenu.tsx:1-60 — confidence: high — unconfirmed
- A-06 The block's EyeOff menu holds Noise / Duplicate / Internal — evidence: web/components/IgnoreMenu.tsx:73-77 — confidence: high — unconfirmed
- A-07 Daemon has `POST /blocks/:id/split` (first_minutes) with NO web caller — evidence: rust/crates/worklog-core/src/daemon.rs:205,1664; grep web/ no hits — confidence: high — unconfirmed
- A-08 Daemon has `POST /blocks/merge`; web only imports it in actions.ts for Duplicate — evidence: daemon.rs:207, web/app/actions.ts:37 — confidence: medium — unconfirmed
- A-09 `runSync(day, dryRun, jiraIssue)` already takes one ticket — evidence: web/app/actions.ts:195 — confidence: high — unconfirmed
- A-10 Day-wide estimate only fills blocks not yet estimated and skips `manual`, so a prompt-regenerated description is not wiped by "Estimate day" — evidence: rust/crates/worklog-core/src/estimate.rs:1-11 — confidence: high — unconfirmed
## Verify
- On the day page, right-click a block with a bad description → "Regenerate with prompt" → type "this was the login bug fix" → the description changes to match; the block's duration and Jira ticket are unchanged. — user, Q7
## Open
