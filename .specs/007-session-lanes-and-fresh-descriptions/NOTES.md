# Notes — 007

Ruling: T001 trimmed existing doc comments in infer_lanes.rs to hold the 400-line cap (contract rule 5) — wording condensed, no logic dropped.
Ruling: T002 shipped `tag_sessions` with a temporary `#[allow(dead_code)]` (not yet called, files: scope); T003 is allowed the one-line deletion of that allow in session_customers.rs when it wires the call, since inline lint suppressions are banned.
Ruling: infer_lanes::build_project_blocks keeps a local lane-KEY map for a run owner's own events, because events_by_key is now folder-keyed for allocation windows (design gap found at T001 brief).
Discovered: on live 2026-09-25 data every vitinn-infra `claude_turn` is titled "prompt" — the prompt text lives in `raw_json` ($.text, RawRecord::ClaudePrompt). Titles + jira_issue resolve no session, so the split never fires (A1 miss). Prompt text alone resolves c84a8921/08ff5212 → Sjúkra, 8dd592aa → APRÓ. — fold into T007
Ruling: T007 appended after approval (Owner away, standing approval for the build). Design §1 said session text = titles + jira_issue; A1 names prompts. T007 makes `load_day_events` carry a `claude_turn`'s prompt text as its InferEvent `title` (only claude_turn; `title` is read elsewhere only for `shell` dedupe and `claude` lifecycle rows). Tool/raw output stays excluded — it mentions both customers in nearly every session. T006 now also waits on T007.
Discovered: dropping a stale description (T004) also re-describes blocks already synced to Tempo / exported; their Tempo/invoice text won't follow. Spec does not exempt them — defer, raise with Owner in PR.
