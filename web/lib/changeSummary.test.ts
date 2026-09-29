import { describe, expect, it } from "bun:test";
import { countBlocks, overlap, summariseChanges, wordDiff } from "./changeSummary";
import type { BlockChange } from "./deildir";

let nextId = 1;
function change(o: Partial<BlockChange>): BlockChange {
  return {
    id: nextId++,
    day: "2026-09-29",
    started_at: "2026-09-29T13:50:00+00:00",
    field: "description",
    old: null,
    new: null,
    source: "claude",
    batch: "b",
    created_at: "2026-09-29T14:00:00Z",
    seen: false,
    ...o,
  };
}

describe("summariseChanges", () => {
  it("nets a rebuild clear + Claude rewrite into one old → new edit", () => {
    const days = summariseChanges([
      change({ old: "Add toggles", new: null, source: "rebuild" }),
      change({ old: null, new: "Build toggles", source: "claude" }),
    ]);
    expect(days).toHaveLength(1);
    expect(days[0].blocks[0].changes).toEqual([
      { kind: "description", old: "Add toggles", new: "Build toggles", sources: ["rebuild", "claude"] },
    ]);
  });

  it("drops a block whose changes cancel out", () => {
    const days = summariseChanges([
      change({ old: "Same", new: null, source: "rebuild" }),
      change({ old: null, new: "Same", source: "claude" }),
    ]);
    expect(days).toEqual([]);
  });

  it("chains customer then deild into one billing change", () => {
    const [day] = summariseChanges([
      change({ field: "customer", old: "APRÓ 100%", new: "Sjúkra 100%" }),
      change({ field: "deild", old: "Sjúkra 100%", new: "Sjúkra·Rekstur 100%", source: "user" }),
      change({ old: null, new: "Investigate retention" }),
    ]);
    expect(day.blocks[0].changes.map((c) => [c.kind, c.old, c.new])).toEqual([
      ["description", null, "Investigate retention"],
      ["billing", "APRÓ 100%", "Sjúkra·Rekstur 100%"],
    ]);
  });

  it("groups by day (newest first) and orders blocks by clock time", () => {
    const days = summariseChanges([
      change({ day: "2026-09-28", started_at: "2026-09-28T09:00:00Z", new: "a" }),
      change({ started_at: "2026-09-29T15:00:00Z", new: "c" }),
      change({ started_at: "2026-09-29T09:00:00Z", new: "b" }),
    ]);
    expect(days.map((d) => d.day)).toEqual(["2026-09-29", "2026-09-28"]);
    expect(days[0].blocks.map((b) => b.changes[0].new)).toEqual(["b", "c"]);
    expect(countBlocks(days)).toBe(3);
  });
});

describe("wordDiff", () => {
  const join = (parts: ReturnType<typeof wordDiff>, keep: string) =>
    parts.filter((p) => p.kind === "same" || p.kind === keep).map((p) => p.text).join("");

  it("marks only the changed words and round-trips both texts", () => {
    const a = "Add user-controlled tool toggles for LibreChat agents";
    const b = "Build per-user tool switches for LibreChat agents";
    const parts = wordDiff(a, b);
    expect(join(parts, "del")).toBe(a);
    expect(join(parts, "add")).toBe(b);
    expect(parts.filter((p) => p.kind === "same").map((p) => p.text.trim())).toContain("for LibreChat agents");
    expect(parts.findIndex((p) => p.kind === "del")).toBeLessThan(parts.findIndex((p) => p.kind === "add"));
  });

  it("overlap is 1 for identical text and low for a full rewrite", () => {
    expect(overlap(wordDiff("same words", "same words"))).toBe(1);
    expect(overlap(wordDiff("completely different", "nothing shared here"))).toBe(0);
  });
});

describe("summariseChanges blanks", () => {
  it("treats an empty-string description like a cleared one", () => {
    const [day] = summariseChanges([change({ old: "Add test", new: "", source: "user" })]);
    expect(day.blocks[0].changes[0].new).toBeNull();
    expect(summariseChanges([change({ old: "", new: null })])).toEqual([]);
  });
});

it("orders sources by last touch", () => {
  const [day] = summariseChanges([
    change({ old: "A", new: null, source: "rebuild" }),
    change({ old: null, new: "B", source: "claude" }),
    change({ old: "B", new: null, source: "rebuild" }),
  ]);
  expect(day.blocks[0].changes[0]).toMatchObject({ old: "A", new: null, sources: ["claude", "rebuild"] });
});
