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
