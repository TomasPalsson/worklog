import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { layout, plantHeight, plantTip, TabGarden } from "./TabGarden";
import { parseTip } from "./tip";

afterEach(cleanup);

const rows = [
  { label: "localhost:3000", value: 157 },
  { label: "github.com", value: 208 },
  { label: "docs.rs", value: 30 },
];

describe("layout", () => {
  it("ranks by minutes, caps at 10, drops zeros, sqrt heights", () => {
    const many = Array.from({ length: 14 }, (_, i) => ({ label: `s${i}.com`, value: i }));
    const L = layout(many);
    expect(L.plants.length).toBe(10);
    expect(L.plants[0].label).toBe("s13.com");
    expect(L.plants.every((p) => p.minutes > 0)).toBe(true);
    const h = layout(rows).plants.map((p) => p.h);
    expect(h[0]).toBeGreaterThan(h[1]);
    expect(h[1]).toBeGreaterThan(h[2]);
    expect(plantHeight(0, 10)).toBe(0);
    expect(plantHeight(10, 0)).toBe(0);
    expect(plantHeight(1e9, 1e9)).toBeCloseTo(7.5);
  });
  it("empty and single", () => {
    expect(layout([]).plants).toEqual([]);
    expect(layout([{ label: "a.com", value: 1 }]).plants.length).toBe(1);
  });
});

describe("plantTip", () => {
  const L = layout(rows).plants;
  it("leader: exact strings", () => {
    const t = plantTip(L[0], rows);
    expect(t.title).toBe("github.com");
    expect(t.sub).toBe("1 heartbeat = 1 minute in a tab");
    expect(t.rows).toEqual([
      ["Time in tab", "3h 28m"],
      ["Share of top sites", "53%"],
      ["Rank", "#1 of 3"],
      ["Heartbeats", "≈ 208"],
    ]);
    expect(t.bar).toEqual({ value: 208, max: 208, label: "100% of the tallest plant" });
    expect(t.note).toBe("More than the other 2 sites combined.");
    const close = [{ label: "a.com", value: 150 }, { label: "b.com", value: 100 }, { label: "c.com", value: 90 }];
    expect(plantTip(layout(close).plants[0], close).note).toBe("1.5× the runner-up, b.com.");
  });
  it("ties are not ranked as a lead", () => {
    const tie = [{ label: "a.com", value: 100 }, { label: "b.com", value: 100 }];
    const ps = layout(tie).plants;
    expect(plantTip(ps[0], tie).note).toBe("Tied with b.com for the tallest.");
    const t = plantTip(ps[1], tie);
    expect(t.note).toBe("Tied with a.com for the tallest.");
    expect(t.rows!.some(([l]) => l === "Behind the tallest")).toBe(false);
  });
  it("localhost runner-up", () => {
    const t = plantTip(L[1], rows);
    expect(t.note).toBe("localhost:3000: 2h 37m of visiting your own work.");
    expect(t.rows).toContainEqual(["Behind the tallest", "51m"]);
  });
  it("localhost as favourite", () => {
    const r = [{ label: "localhost", value: 157 }];
    expect(plantTip(layout(r).plants[0], r).note).toBe("localhost: you were your own favourite website for 2h 37m.");
  });
  it("edge: single site, dominant site, tiny share", () => {
    const one = [{ label: "a.com", value: 5 }];
    expect(plantTip(layout(one).plants[0], one).note).toContain("one-plant");
    const big = [{ label: "a.com", value: 1000 }, { label: "b.com", value: 10 }];
    expect(plantTip(layout(big).plants[0], big).note).toBe("More than the other 1 site combined.");
    const huge = [{ label: "a.com", value: 1e6 }, { label: "b.com", value: 1 }];
    const t = plantTip(layout(huge).plants[1], huge);
    expect(t.rows![1]).toEqual(["Share of top sites", "<1%"]);
    expect(t.rows).toContainEqual(["Heartbeats", "≈ 1"]);
  });
});

describe("TabGarden render", () => {
  it("tips each plant, no <title>, keyboard reachable", () => {
    const { container } = render(<TabGarden rows={rows} />);
    const marks = container.querySelectorAll("g[data-stip]");
    expect(marks.length).toBe(3);
    for (const m of marks) {
      expect(m.getAttribute("tabindex")).toBe("0");
      expect(parseTip(m.getAttribute("data-stip"))).not.toBeNull();
      expect(m.querySelector("title")).toBeNull();
    }
    expect(container.querySelector("title")).toBeNull();
    expect(container.textContent).toContain("github.com grew tallest: 3h 28m of browsing");
    expect(container.querySelector("svg")!.getAttribute("role")).toBe("img");
    expect(container.textContent).toContain("height = time in the tab");
  });
  it("empty state", () => {
    const { container } = render(<TabGarden rows={[]} />);
    expect(container.textContent).toContain("The garden is bare");
    expect(container.querySelectorAll("[data-stip]").length).toBe(0);
  });
});
