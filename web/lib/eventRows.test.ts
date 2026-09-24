import { describe, expect, it } from "bun:test";
import type { Event } from "./types";
import { eventRows } from "./eventRows";

let nextId = 1;
function ev(source: string, hhmm: string, title: string, extra: Partial<Event> = {}): Event {
  return {
    id: nextId++,
    source,
    source_id: `${source}-${nextId}`,
    started_at: `2026-09-23T${hhmm}:00Z`,
    ended_at: null,
    duration_seconds: null,
    title,
    details: null,
    repo: null,
    project_path: "/Users/x/Desktop/Work/genai-infra",
    jira_issue: null,
    session_id: null,
    tempo_worklog_id: null,
    raw_json: null,
    ...extra,
  };
}

describe("eventRows", () => {
  it("folds a stretch of Claude working into one row with everything it did", () => {
    const rows = eventRows([
      ev("claude_work", "03:05", "claude working", { details: "branch apro-fix · Bash ×2" }),
      ev("claude_work", "03:05", "claude working", { details: "branch apro-fix · Bash ×2" }), // copied session
      ev("claude_work", "03:20", "claude working", { details: "branch apro-fix · Bash, Edit · edited src/a.tf" }),
      ev("claude_work", "03:35", "claude working", { details: "branch apro-fix · Edit · edited src/b.tf" }),
    ]);
    expect(rows).toHaveLength(1);
    const r = rows[0];
    expect(r.kind).toBe("claude");
    expect(r.time).toBe("03:05–03:36");
    expect(r.text).toBe("Claude working · 3 min");
    expect(r.detail).toBe("branch apro-fix · Bash ×3, Edit ×2 · edited src/a.tf, src/b.tf");
  });

  it("shows the prompt you typed, and a plain line when the snippet is gone", () => {
    const rows = eventRows([
      ev("claude_turn", "09:00", "prompt", { snippet: "fix the login bug" }),
      ev("claude_turn", "09:05", "prompt"),
    ]);
    expect(rows.map((r) => [r.kind, r.text])).toEqual([
      ["prompt", "fix the login bug"],
      ["prompt", "You wrote a prompt"],
    ]);
  });

  it("folds back-to-back shell commands into one row of program names", () => {
    const rows = eventRows([
      ev("shell", "10:00", "git"),
      ev("shell", "10:01", "cargo"),
      ev("shell", "10:02", "git"),
      ev("claude_turn", "10:03", "prompt", { snippet: "why" }),
    ]);
    expect(rows[0].kind).toBe("shell");
    expect(rows[0].text).toBe("Ran git ×2, cargo");
    expect(rows[1].kind).toBe("prompt");
  });

  it("breaks a Claude stretch at a long pause", () => {
    const rows = eventRows([
      ev("claude_work", "09:00", "claude working"),
      ev("claude_work", "09:45", "claude working"),
    ]);
    expect(rows).toHaveLength(2);
  });
});
