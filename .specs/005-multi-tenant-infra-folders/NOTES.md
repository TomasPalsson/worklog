Ruling: T001 touched billing.rs, daemon.rs and three routing_*_test.rs outside its files: — one `multi_tenant: false,` line per FolderMap literal, forced by the new non-Option field; no logic change. Accepted.
Ruling: T001 loosened `schema_version == 12` to `>= 12` in the v12 test, matching db.rs's floor pattern; the new v13 test asserts the exact version. Accepted.
