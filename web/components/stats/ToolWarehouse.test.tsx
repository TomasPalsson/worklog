import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { layout, niceUnit, pegCount, shortTool, stackCrates, ToolWarehouse } from "./ToolWarehouse";

afterEach(cleanup);

describe("shortTool", () => {
  it("shortens mcp names", () => {
    expect(shortTool("mcp__plugin_chrome-devtools-mcp_chrome-devtools__evaluate_script")).toBe("devtools·evaluate_script");
    expect(shortTool("mcp__claude_ai_Gmail__x")).toBe("gmail·x");
    expect(shortTool("Bash")).toBe("Bash");
  });
});

describe("stacking", () => {
  it("niceUnit keeps the biggest stack within 24 crates", () => {
    expect(niceUnit(11778)).toBe(500);
    expect(niceUnit(10)).toBe(1);
    expect(Math.ceil(1e9 / niceUnit(1e9))).toBeLessThanOrEqual(24);
  });
  it("fills 2x2 then climbs, with a half-height remainder", () => {
    const c = stackCrates(2250, 500); // 5 crates, last partial
    expect(c.length).toBe(5);
    expect(c.filter((k) => k.z === 0).length).toBe(4);
    expect(c.at(-1)!.h).toBe(0.5);
    expect(stackCrates(1000, 500).every((k) => k.h === 1)).toBe(true);
    expect(stackCrates(0, 5)).toEqual([]);
  });
  it("pegCount: one per 1,000, min 1, capped", () => {
    expect(pegCount(11372)).toEqual({ pegs: 11, per: 1000 });
    expect(pegCount(40)).toEqual({ pegs: 1, per: 1000 });
    expect(pegCount(0).pegs).toBe(0);
    expect(pegCount(1_000_000).pegs).toBeLessThanOrEqual(16);
  });
  it("row gap keeps back-row name plates clear of the tallest front stack", () => {
    const l = layout(Array.from({ length: 8 }, (_, i) => ({ label: `t${i}`, value: 12000 - i })));
    const front = Math.max(...l.pallets.slice(4).map((p) => p.height));
    expect(l.sy).toBeGreaterThan(4.7 + front);
    expect(l.pallets[4].gy).toBeCloseTo(l.sy);
    expect(layout([{ label: "a", value: 5 }]).pallets[0].gy).toBe(0);
  });
  it("layout caps at 8 tools and has finite bounds", () => {
    const l = layout(Array.from({ length: 12 }, (_, i) => ({ label: `t${i}`, value: 100 - i })));
    expect(l.pallets.length).toBe(8);
    expect(Object.values(l.bounds).every(Number.isFinite)).toBe(true);
  });
});

describe("ToolWarehouse", () => {
  const tools = [
    { label: "Bash", value: 11778 },
    { label: "mcp__claude_ai_Gmail__x", value: 300 },
    { label: "Read", value: 3000 },
  ];
  it("headline, legend, aria", () => {
    const { container } = render(<ToolWarehouse tools={tools} helpers={[{ label: "Explore", value: 11372 }]} />);
    const t = container.textContent!;
    expect(t).toContain("Bash runs the warehouse: 11,778 calls (78% of your top tools)");
    expect(t).toContain("11,372 subagents, 1 peg = 1,000");
    expect(t).toContain("1 crate = 500 calls");
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("role")).toBe("img");
    expect(svg.getAttribute("aria-label")).toContain("11,778 calls");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("uses the report totals when the ranked rows are truncated", () => {
    const { container } = render(
      <ToolWarehouse tools={[{ label: "Bash", value: 11778 }, { label: "Read", value: 3000 }]} helpers={[{ label: "Explore", value: 5000 }]} totalCalls={16134} totalHelpers={11372} />,
    );
    const t = container.textContent!;
    expect(t).toContain("Bash runs the warehouse: 11,778 calls (73% of all)");
    expect(t).toContain("11,372 subagents, 1 peg = 1,000");
    expect(container.querySelector("svg")!.getAttribute("aria-label")).toContain("73% of all");
  });
  it("empty state", () => {
    const { container } = render(<ToolWarehouse tools={[]} helpers={[]} />);
    expect(container.textContent).toContain("nothing built yet");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("single tool, no helpers, huge value", () => {
    const { container } = render(<ToolWarehouse tools={[{ label: "Bash", value: 1e9 }]} helpers={[]} />);
    expect(container.textContent).toContain("(100% of your top tools)");
    expect(container.textContent).not.toContain("subagents ·");
    expect(container.innerHTML).not.toContain("NaN");
  });
});

import { doorTip, forkliftTip, palletTip, pegsTip, ranked } from "./WarehouseTips";
import { parseTip } from "./tip";

describe("warehouse tips", () => {
  const list = ranked([
    { label: "Read", value: 3000 },
    { label: "Bash", value: 11778 },
    { label: "mcp__claude_ai_Gmail__x", value: 300 },
    { label: "Zero", value: 0 },
  ]);
  it("top pallet", () => {
    const t = palletTip(list[0], list, 16134, 500, 10, "red");
    expect(t.title).toBe("Bash");
    expect(t.sub).toBe("#1 of 3 tools");
    expect(t.rows).toEqual([
      ["Calls", "11,778"],
      ["Share of all calls", "73%"],
      ["Crates", "24 (1 crate = 500)"],
      ["Per worked day", "1177.8 calls"],
      ["Gap to next", "8,778 ahead of Read"],
    ]);
    expect(t.bar).toEqual({ value: 11778, max: 11778, label: "the busiest tool" });
    expect(t.note).toBe("Bash alone outruns the next 2 tools combined (3.6×).");
    expect(t.accent).toBe("red");
  });
  it("middle and last pallets, no days", () => {
    const mid = palletTip(list[1], list, 16134, 500);
    expect(mid.sub).toBe("#2 of 3 tools");
    expect(mid.rows!.map((r) => r[0])).not.toContain("Per worked day");
    expect(mid.note).toBe("Read outruns the 1 tool below it combined (10.0×).");
    expect(mid.bar!.label).toBe("vs Bash");
    const last = palletTip(list[2], list, 16134, 500);
    expect(last.rows!.at(-1)).toEqual(["Gap to next", "last of the shown tools"]);
    expect(last.note).toBe("gmail·x brings up the rear.");
  });
  it("close race, single tool, tiny share", () => {
    const two = ranked([{ label: "A", value: 10 }, { label: "B", value: 9 }, { label: "C", value: 5 }]);
    expect(palletTip(two[0], two, 24, 1).note).toBe("Only 1 calls separate A from B.");
    const one = [{ label: "Solo", value: 1 }];
    expect(palletTip(one[0], one, 1_000_000, 1).rows![1]).toEqual(["Share of all calls", "<1%"]);
    expect(palletTip(one[0], one, 1, 1).note).toBe("Solo is the only tool in the warehouse.");
    expect(palletTip({ label: "x", value: 5 }, [], 0, 1).rows![1][1]).toBe("0%");
  });
  it("forklift, pegs, door", () => {
    const f = forkliftTip(list[0], 16134, 500, 10);
    expect(f.title).toBe("Forklift: hauling Bash");
    expect(f.note).toBe("73% of every tool call is Bash, so the forklift never clocks off.");
    expect(f.bar).toEqual({ value: 11778, max: 16134, label: "of all tool calls" });
    const pegs = pegsTip([{ label: "Explore", value: 11000 }, { label: "Plan", value: 372 }, { label: "none", value: 0 }], 11372, 1000, 10);
    expect(pegs.sub).toBe("1 peg = 1,000 delegations");
    expect(pegs.rows).toEqual([["Explore", "11,000"], ["Plan", "372"], ["Total", "11,372 subagents"], ["Per worked day", "1137.2"]]);
    expect(pegs.note).toBe("You delegated 1137.2 times a day.");
    expect(pegsTip([], 5, 1000).note).toBe("You handed work to 5 helpers in this range.");
    const d = doorTip(16134, 3, list[0], 10);
    expect(d.rows).toEqual([["Tool calls", "16,134"], ["Top tools shown", "3"], ["Per worked day", "1613.4 calls"], ["Most used", "Bash (73%)"]]);
    expect(d.note).toBe("Roughly 1613.4 deliveries every worked day.");
    expect(doorTip(0, 0, undefined).note).toBe("0 deliveries in this range.");
  });
  it("marks carry data-stip, no native <title>, keyboard reachable", () => {
    const { container } = render(
      <ToolWarehouse
        tools={[{ label: "Bash", value: 11778 }, { label: "Read", value: 3000 }]}
        helpers={[{ label: "Explore", value: 11372 }]}
        totalCalls={16134}
        totalHelpers={11372}
        daysWorked={10}
      />,
    );
    expect(container.querySelector("title")).toBeNull();
    const tips = [...container.querySelectorAll("[data-stip]")].map((e) => parseTip(e.getAttribute("data-stip"))!.title);
    expect(tips).toContain("Bash");
    expect(tips).toContain("Forklift: hauling Bash");
    expect(tips).toContain("Subagents");
    expect(tips).toContain("The warehouse door");
    const pallet = container.querySelector(".sx-warehouse-stack")!;
    expect(pallet.getAttribute("tabindex")).toBe("0");
    expect(pallet.getAttribute("aria-label")).toBe("Bash, 11,778 calls");
    expect(parseTip(pallet.getAttribute("data-stip"))!.rows![3]).toEqual(["Per worked day", "1177.8 calls"]);
    expect(container.innerHTML).not.toContain("NaN");
  });
});
