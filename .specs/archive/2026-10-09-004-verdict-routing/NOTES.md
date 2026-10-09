Ruling: 2026-09-23 build started without an Approved: line — the owner's /goal message says "assume that I approve of everything you do"; the approval line itself stays the owner's to write — cost if wrong: branch is local-only, reset to eb435ef undoes it
Discovered: T001 had to adapt routing_test.rs (T003-owned) so the new Guess/RouteRule compile — mechanical only, no assertion changed — fold into T003
Discovered: T001 per-task review skipped (pure rename + contract); whole-branch review at gates covers it — defer
Discovered: daemon froze (health 000, 0% CPU) — GET/POST /settings read the macOS keychain on async workers; a pending keychain permission dialog for a freshly built binary blocked one worker per call until all 4 were stuck. Fixed: settings_off_runtime() spawn_blocking (1f8013d). Pre-existing on main — fold (done)
Discovered: daemon serialises the classifier state with sorted keys (serde_json Map), so the model sees {"container","details","source","title"}; thresholds must be measured in that order — note
Discovered: real-day run with the published calibrator: right picks ×1.06–×1.23 vs abstain, a wrong pick (The Morning Checkup → claude-3p-config) ×1.16 — ratios alone cannot separate; amended with FR-10 exact repo/path rule + default ×1.20 → T007
Discovered: T002 parked finding (single-option Noul path untested, keys guessed) — resolved: real Noul keys are true/false/__insufficient_evidence__, but the model favours a lone option whatever the text ("pool" 0.48 true), so a lone option is never guessed (fbefc11)
Discovered: daemon::tests::routing_status_reports_last_heartbeat_and_last_slack fails whenever a real Verdict helper listens on 127.0.0.1:9324 — test assumes the port is free — defer
Discovered: daemon.rs is 4301 lines (size guard max 400), pre-existing ~4214 — defer (own feature)
Discovered: B6 test uses 0.5 and 9.0, not the spec's 0.9/5.5 boundary values — defer
