import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { SplitDonut, segTip } from "./SplitDonut";

afterEach(cleanup);

const mk = (rows: { label: string; value: number }[]) => {
  const total = rows.reduce((a, r) => a + r.value, 0);
  return rows.map((r) => ({ ...r, hatch: r.label === "none", color: "var(--sage)", share: total ? r.value / total : 0 }));
};

describe("segTip ties", () => {
  it("equal slices say level", () => {
    const a = { label: "A", value: 5, share: 0.5, hatch: false, color: "x" };
    const b = { ...a, label: "B" };
    expect(segTip(a, [a, b], "count", "T").note).toBe("Level with B.");
  });
});

describe("segTip", () => {
  it("majority slice", () => {
    const items = mk([{ label: "event", value: 7200 }, { label: "auto", value: 1800 }, { label: "other", value: 1800 }]);
    const t = segTip(items[0], items, "seconds", "Origin");
    expect(t.title).toBe("event");
    expect(t.sub).toBe("Origin · 1st of 3");
    expect(t.rows).toEqual([["Value", "2h"], ["Share", "67%"], ["Rank", "1 of 3"], ["Of the rest", "1h"]]);
    expect(t.note).toBe("More than all the other slices combined (1h).");
    expect(t.accent).toBe("var(--sage)");
  });
  it("minor, hatched and zero slices", () => {
    const items = mk([{ label: "event", value: 40 }, { label: "auto", value: 30 }, { label: "none", value: 30 }, { label: "x", value: 0 }]);
    expect(segTip(items[0], items, "count", "O").note).toBe("The biggest slice, but not a majority.");
    expect(segTip(items[1], items, "count", "O").note).toBe("1.3× smaller than event.");
    expect(segTip(items[2], items, "count", "O").accent).toBe("var(--fg-subtle)");
    expect(segTip(items[3], items, "count", "O").note).toBe("Nothing in this slice yet.");
  });
  it("single slice owns the ring", () => {
    const items = mk([{ label: "a", value: 5 }]);
    expect(segTip(items[0], items, "count", "T").note).toBe("The whole ring: nothing else here.");
  });
});

describe("SplitDonut tips", () => {
  it("tips segments (one tab stop each) and legend rows, no native titles", () => {
    const { container } = render(<SplitDonut title="T" unit="count" rows={[{ label: "a", value: 7 }]} />);
    expect(container.querySelectorAll("path.sr-seg[data-stip]").length).toBe(2);
    expect(container.querySelectorAll("path.sr-seg[tabindex]").length).toBe(1);
    expect(container.querySelectorAll("li[data-stip]").length).toBe(1);
    expect(container.querySelectorAll("title,[title]").length).toBe(0);
  });
});
