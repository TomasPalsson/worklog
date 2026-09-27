# Notes — spec 006

- Discovered: web/lib/types.ts is over the 400-line guard; spec-006 TS types live in web/lib/clues_contract.ts instead (T001) — defer
- Ruling: a day with no keyed project event at all (only folderless + calendar) still gap-clusters folderless events into blocks as before (T004) — FR-09 is enforced whenever any project span exists; forcing it on pure-folderless days would erase unrouted browser/Jira-only days and rewrite ~40 existing infer tests — cost if wrong: such a day shows unassigned blocks the owner dismisses by hand.
