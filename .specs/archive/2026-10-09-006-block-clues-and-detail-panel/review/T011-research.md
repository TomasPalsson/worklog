# T011 research — how helper activity looks on disk (2026-09-27, verified on real data)

1. Subagents (Task/Agent tool): `~/.claude/projects/<dir>/<sessionId>/subagents/agent-<agentId>.jsonl`
   + `agent-<agentId>.meta.json`. Every line: `sessionId` = PARENT session id, `agentId`,
   `isSidechain: true`, `promptId` = parent turn. meta.json: `agentType`, `description`,
   `toolUseId` (== the parent's `Agent` tool_use id), `spawnDepth`, optional `parentAgentId`,
   `teamName` + `taskKind: "in_process_teammate"` for teammates.
   The current collector only reads `<dir>/*.jsonl`, so these are never read today.
2. Workflow tasks: `<dir>/<sessionId>/subagents/workflows/wf_<runId>/agent-<id>.jsonl` (+ thin
   meta `{"agentType":"workflow-subagent"}`, plus a `journal.jsonl` to ignore). Same parent
   `sessionId`/`promptId`.
3. No `isSidechain: true` lines exist in main session files any more — sidechains live only in
   `subagents/`.
4. `sessionKind: "bg"` is on every line — NOT a background-job marker. Background jobs are
   `~/.claude/jobs/<id8>/state.json` with `sessionId` (full UUID == the job's own transcript
   stem). The job session's own transcript is a normal top-level session; there is no link to
   a launching session in the data.
5. Inter-session messages: `type:"user"` lines whose string content holds
   `<teammate-message teammate_id="…">…</teammate-message>`, or `type:"attachment"` lines with
   `attachment.type:"queued_command"`, `attachment.prompt` = `<agent-message from="<agentId>">…`
   and `attachment.origin = {"kind":"peer","from":"<agentId>"}`. `origin.kind` values seen:
   human, peer, task-notification, coordinator, auto-continuation.
