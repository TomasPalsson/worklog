import { describe, expect, it } from "bun:test";
import type { DetailRow, RawRecord } from "./clues_contract";
import { buildTimeline, filterTimeline, sourcesPresent, type SourceKind, type TimelineItem } from "./detailRows";

let nextId = 1;
function row(source: string, hhmm: string, title: string, extra: Partial<DetailRow> = {}): DetailRow {
  return {
    id: nextId++,
    source,
    started_at: `2026-09-23T${hhmm}:00Z`,
    title,
    details: null,
    repo: null,
    project_path: null,
    session_id: null,
    jira_issue: null,
    raw: null,
    ...extra,
  };
}

function toolRaw(files: string[]): RawRecord {
  return { kind: "claude_tool", session_id: "s1", tool: "Edit", input: {}, output: null, output_cut_bytes: 0, files };
}

function asSession(item: TimelineItem) {
  if (item.kind !== "session") throw new Error("expected a session item");
  return item;
}

/** Total rows folded into `items` — used to prove nothing is dropped. */
function countRows(items: TimelineItem[]): number {
  let n = 0;
  for (const item of items) {
    if (item.kind === "event") {
      n += 1;
      continue;
    }
    for (const turn of item.turns) n += (turn.prompt ? 1 : 0) + turn.tools.length;
    for (const helper of item.helpers) n += helper.rows.length;
    n += item.messages.length + item.work.length;
  }
  return n;
}

describe("buildTimeline", () => {
  it("folds a session into prompt -> tools -> deduped files", () => {
    const prompt = row("claude_turn", "09:00", "prompt", { session_id: "s1" });
    const tool1 = row("claude_tool", "09:01", "tool", { session_id: "s1", raw: toolRaw(["a.ts"]) });
    const tool2 = row("claude_tool", "09:02", "tool", { session_id: "s1", raw: toolRaw(["b.ts", "a.ts"]) });

    const timeline = buildTimeline([prompt, tool1, tool2]);

    expect(timeline).toHaveLength(1);
    const session = asSession(timeline[0]);
    expect(session.turns).toHaveLength(1);
    expect(session.turns[0].prompt?.id).toBe(prompt.id);
    expect(session.turns[0].tools.map((t) => t.id)).toEqual([tool1.id, tool2.id]);
    expect(session.turns[0].files).toEqual(["a.ts", "b.ts"]);
  });

  it("keeps two sessions separate", () => {
    const p1 = row("claude_turn", "09:00", "p1", { session_id: "s1" });
    const p2 = row("claude_turn", "09:05", "p2", { session_id: "s2" });

    const timeline = buildTimeline([p1, p2]);

    expect(timeline).toHaveLength(2);
    expect(timeline.map((t) => asSession(t).sessionId)).toEqual(["s1", "s2"]);
  });

  it("nests helper rows under their parent session, grouped by title", () => {
    const prompt = row("claude_turn", "09:00", "prompt", { session_id: "s1" });
    const helper1 = row("claude_helper", "09:01", "Subagent: foo", {
      session_id: "s1",
      raw: { kind: "helper", parent_session_id: "s1", helper_kind: "subagent", summary: "did foo" },
    });
    const helper2 = row("claude_helper", "09:02", "Subagent: foo", {
      session_id: "s1",
      raw: { kind: "helper", parent_session_id: "s1", helper_kind: "subagent", summary: "did more foo" },
    });
    const helper3 = row("claude_helper", "09:03", "Subagent: bar", {
      session_id: "s1",
      raw: { kind: "helper", parent_session_id: "s1", helper_kind: "teammate", summary: "did bar" },
    });

    const timeline = buildTimeline([prompt, helper1, helper2, helper3]);

    expect(timeline).toHaveLength(1);
    const session = asSession(timeline[0]);
    expect(session.helpers).toHaveLength(2);
    expect(session.helpers[0]).toMatchObject({ title: "Subagent: foo", helperKind: "subagent" });
    expect(session.helpers[0].rows.map((r) => r.id)).toEqual([helper1.id, helper2.id]);
    expect(session.helpers[1]).toMatchObject({ title: "Subagent: bar", helperKind: "teammate" });
    expect(session.helpers[1].rows.map((r) => r.id)).toEqual([helper3.id]);
  });

  it("puts tools before the first prompt in a null-prompt leading turn", () => {
    const leadingTool = row("claude_tool", "09:00", "tool", { session_id: "s1", raw: toolRaw(["x.ts"]) });
    const prompt = row("claude_turn", "09:01", "prompt", { session_id: "s1" });
    const laterTool = row("claude_tool", "09:02", "tool", { session_id: "s1", raw: toolRaw(["y.ts"]) });

    const timeline = buildTimeline([leadingTool, prompt, laterTool]);

    const session = asSession(timeline[0]);
    expect(session.turns).toHaveLength(2);
    expect(session.turns[0].prompt).toBeNull();
    expect(session.turns[0].tools.map((t) => t.id)).toEqual([leadingTool.id]);
    expect(session.turns[0].files).toEqual(["x.ts"]);
    expect(session.turns[1].prompt?.id).toBe(prompt.id);
    expect(session.turns[1].tools.map((t) => t.id)).toEqual([laterTool.id]);
  });

  it("drops no rows across a mix of plain events and a full session", () => {
    const rows = [
      row("shell", "08:00", "ls"),
      row("claude_turn", "09:00", "prompt", { session_id: "s1" }),
      row("claude_tool", "09:01", "tool", { session_id: "s1", raw: toolRaw(["a.ts"]) }),
      row("claude_tool", "09:02", "tool", { session_id: "s1", raw: toolRaw(["b.ts"]) }),
      row("claude_helper", "09:03", "Subagent: foo", {
        session_id: "s1",
        raw: { kind: "helper", parent_session_id: "s1", helper_kind: "subagent", summary: "x" },
      }),
      row("claude_helper", "09:04", "Subagent: bar", {
        session_id: "s1",
        raw: { kind: "helper", parent_session_id: "s1", helper_kind: "background_job", summary: "y" },
      }),
      row("claude_message", "09:05", "message", { session_id: "s1" }),
      row("claude_work", "09:06", "claude working", { session_id: "s1" }),
      row("git_reflog", "09:10", "checkout"),
    ];

    const timeline = buildTimeline(rows);

    expect(countRows(timeline)).toBe(rows.length);
  });

  it("does not throw on a null or malformed raw, and contributes no files", () => {
    const prompt = row("claude_turn", "09:00", "prompt", { session_id: "s1" });
    const nullRawTool = row("claude_tool", "09:01", "tool", { session_id: "s1", raw: null });

    expect(() => buildTimeline([prompt, nullRawTool])).not.toThrow();
    const session = asSession(buildTimeline([prompt, nullRawTool])[0]);
    expect(session.turns[0].tools.map((t) => t.id)).toEqual([nullRawTool.id]);
    expect(session.turns[0].files).toEqual([]);
  });
});

describe("filterTimeline / sourcesPresent", () => {
  const rows = [
    row("shell", "09:00", "ls"),
    row("git_reflog", "09:01", "checkout"),
    row("claude_turn", "09:02", "prompt", { session_id: "s1" }),
  ];
  const timeline = buildTimeline(rows);

  it("keeps only shell items when filtered to shell", () => {
    const filtered = filterTimeline(timeline, new Set<SourceKind>(["shell"]));
    expect(filtered).toHaveLength(1);
    expect(filtered[0].kind).toBe("event");
    expect((filtered[0] as { source: SourceKind }).source).toBe("shell");
  });

  it("restores the full timeline when every present source is enabled", () => {
    const restored = filterTimeline(timeline, new Set(sourcesPresent(timeline)));
    expect(restored).toEqual(timeline);
  });

  it("returns nothing for an empty enabled set", () => {
    expect(filterTimeline(timeline, new Set())).toEqual([]);
  });
});
