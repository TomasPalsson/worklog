Ruling: T001 touched billing.rs, daemon.rs and three routing_*_test.rs outside its files: — one `multi_tenant: false,` line per FolderMap literal, forced by the new non-Option field; no logic change. Accepted.
Ruling: T001 loosened `schema_version == 12` to `>= 12` in the v12 test, matching db.rs's floor pattern; the new v13 test asserts the exact version. Accepted.
Ruling: T005 dev used CC_NO_SIZE_GUARD=1 on billing.rs edits (pre-existing oversize, accepted in design Complexity Tracking); the fix commit ran without the bypass. Verkefni now only kept on the slice whose customer matches the folder pin (0f80a1a).
Ruling: T008 may extend web/lib/tenants.ts and web/app/tenant-actions.ts (T007's files) — its contract says its calls live there.
Discovered: folderSavePayload dropped multi_tenant, so editing a folder cleared its flag — fold in (fix dispatched, billingRegistryDrafts.ts)
Discovered: UI review on sandbox data — fallback slice showed "Unresolved" though billing uses the folder pin; split editor "Add customer" listed only tenant customers and could save a share keyed "Unresolved"; editor and tenant list layout too wide/misaligned — fold in (one polish dispatch)
Ruling: the polish pass may import fetchBillingRegistry from web/app/actions.ts and edit globals.css and daemon_tenants.rs — needed for the fixes above.
Discovered: branch review (review-diff, 1 kept ≥80, 4 dropped) — customer-slices route resolved fallback from description only while billing also reads the ticket summary; preview and export could disagree — fold in (gate fix dispatch, shared resolver in billing.rs)
Discovered: converge — B10 (shares survive re-infer / dropped on moved start) had no test — fold in (same gate fix dispatch)
Ruling: CHK001 files: pointed at ExportPanel.tsx, which the feature never needed to change; repointed to BlockCustomerSplit.tsx (the UI the check exercises).
