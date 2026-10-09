import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { formatDuration } from "@/lib/format";
import { buildFlow, DayFlow, describeFlow, layoutFlow, type FlowBlock } from "./DayFlow";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const blk = (seconds: number, ticket: string | null, o: Partial<FlowBlock> = {}): FlowBlock => ({
  seconds,
  kind: "work",
  ticket,
  sources: [{ source: "claude_prompt", n: 3 }, { source: "github_commit", n: 1 }],
  ...o,
});

describe("buildFlow", () => {
  it("splits a block across sources by event count and conserves seconds", () => {
    const f = buildFlow([blk(4000, "A-1")], null);
    const claude = f.nodes.find((n) => n.id === "s:Claude")!;
    expect(claude.seconds).toBe(3000);
    expect(f.nodes.find((n) => n.id === "s:git")!.seconds).toBe(1000);
    expect(f.total).toBe(4000);
    expect(f.hasBilling).toBe(false);
  });

  it("keeps top 6 tickets, groups the rest, unassigned stays separate", () => {
    const blocks = ["A", "B", "C", "D", "E", "F", "G", "H"].map((k, i) => blk(1000 + i, k));
    blocks.push(blk(500, null));
    const f = buildFlow(blocks, null);
    const t = f.nodes.filter((n) => n.col === 2);
    expect(t.map((n) => n.label)).toEqual(["H", "G", "F", "E", "D", "C", "other", "unassigned"]);
    expect(t.find((n) => n.label === "other")!.seconds).toBe(2001);
    expect(t.find((n) => n.label === "unassigned")!.hatch).toBe(true);
  });

  it("personal/ignored stop at the blocks column; sources-less falls to other", () => {
    const f = buildFlow(
      [blk(600, null, { kind: "personal", sources: [] }), blk(0, "X"), blk(300, null, { kind: "ignored" })],
      null,
    );
    expect(f.nodes.some((n) => n.col >= 2)).toBe(false);
    expect(f.nodes.find((n) => n.id === "s:other")!.seconds).toBe(600);
    expect(f.total).toBe(900);
  });

  it("billing column: classes, unknown for unmapped and unassigned", () => {
    const f = buildFlow([blk(3600, "A"), blk(1800, "B"), blk(900, null)], { A: "billable" });
    expect(f.hasBilling).toBe(true);
    const b = Object.fromEntries(f.nodes.filter((n) => n.col === 3).map((n) => [n.label, n.seconds]));
    expect(b).toEqual({ billable: 3600, unknown: 2700 });
  });
});

describe("layoutFlow", () => {
  it("heights are proportional with a shared scale; drops sources on request", () => {
    const f = buildFlow([blk(3600, "A", { sources: [{ source: "slack", n: 1 }] }), blk(1800, "B", { sources: [{ source: "slack", n: 1 }] })], null);
    const L = layoutFlow(f, 640, true);
    const a = L.nodes.find((n) => n.id === "t:A")!;
    const b = L.nodes.find((n) => n.id === "t:B")!;
    expect(a.h / b.h).toBeCloseTo(2, 5);
    expect(L.ribbons.length).toBe(f.links.length);
    expect(layoutFlow(f, 360, false).nodes.some((n) => n.col === 0)).toBe(false);
  });

  it("keeps last-column labels inside the viewBox", () => {
    const f = buildFlow([blk(37800, "PROJECT-1234")], { "PROJECT-1234": "not_billable" });
    for (const w of [640, 360]) {
      const n = layoutFlow(f, w, true).nodes.find((x) => x.col === 3)!;
      const text = `${n.label} ${formatDuration(n.seconds)}`;
      expect(n.x + 8 + 4 + text.length * 6.2).toBeLessThanOrEqual(w);
    }
  });
});

describe("DayFlow", () => {
  it("renders nothing for an empty day", () => {
    expect(render(<DayFlow blocks={[]} billing={null} />).container.innerHTML).toBe("");
  });

  it("is collapsed with a summary, and opens to labelled graphics", () => {
    const blocks = [blk(7200, "A-1"), blk(1800, null)];
    const { container, getByText } = render(<DayFlow blocks={blocks} billing={null} />);
    expect(container.querySelector("svg")).toBeNull();
    expect(getByText("2h 30m")).toBeTruthy();
    const d = container.querySelector("details")!;
    d.open = true;
    fireEvent(d, new Event("toggle"));
    const label = container.querySelector("svg")!.getAttribute("aria-label")!;
    expect(label).toContain("2h 30m in total");
    expect(label).toContain("work to A-1 2h");
    expect(localStorage.getItem("worklog.dayflow.open")).toBe("1");
  });

  it("defaultOpen starts open and never writes the remembered preference", () => {
    for (const stored of ["1", "0"]) {
      localStorage.setItem("worklog.dayflow.open", stored);
      const { container, unmount } = render(<DayFlow blocks={[blk(3600, "A-1")]} billing={null} defaultOpen />);
      expect(container.querySelector("svg")).not.toBeNull();
      const d = container.querySelector("details")!;
      d.open = stored === "0";
      fireEvent(d, new Event("toggle"));
      d.open = stored !== "0";
      fireEvent(d, new Event("toggle"));
      expect(localStorage.getItem("worklog.dayflow.open")).toBe(stored);
      unmount();
    }
  });

  it("describeFlow omits source flows when sources are hidden", () => {
    const f = buildFlow([blk(3600, "A")], null);
    expect(describeFlow(f, false)).not.toContain("Claude to");
  });
});
