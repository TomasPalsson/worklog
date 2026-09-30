Discovered: popup lastHeartbeat can be up to 60 s stale and background sends no tick on startup, so a fresh popup may show a stale Not counted / Recording — defer
Discovered: web-ext lint warns manifest.json lacks data_collection_permissions — defer
Discovered: design:vary roll wrote .vary/ and added it to .gitignore (left uncommitted, outside the plan) — defer
Ruling: minutes_today hand-rolls the local-day UTC window instead of tz::utc_window_for_local_day — kept — the helper reads the global day_offset() with no offset parameter, and the design contract passes offset explicitly for testability — cost if wrong: two copies of an 8-line window calc to keep in sync
