import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { parseTip } from "./tip";
import { ShellKeycaps, ellipsize, keyColor, keyHeight, keyTip, layout } from "./ShellKeycaps";

afterEach(cleanup);

const rows = [
  { label: "git", value: 150 },
  { label: "cd", value: 401 },
  { label: "ls", value: 90 },
  { label: "zero", value: 0 },
];

describe("ShellKeycaps helpers", () => {
  it("ellipsizes", () => {
    expect(ellipsize("git", 5)).toBe("git");
    expect(ellipsize("kubectl", 4)).toBe("kub…");
    expect(ellipsize("kubectl", 1)).toBe("…");
    expect(ellipsize("x", 0)).toBe("");
  });
  it("height scales with count, never below the floor", () => {
    expect(keyHeight(0, 0)).toBe(0.5);
    expect(keyHeight(401, 401)).toBeCloseTo(2.4);
    expect(keyHeight(1, 1_000_000)).toBeLessThan(0.51);
    expect(keyHeight(9e9, 1)).toBeCloseTo(2.4);
  });
  it("colours by rank band", () => {
    expect([1, 2, 5, 9].map(keyColor)).toEqual(["var(--amber)", "var(--sage)", "var(--violet)", "var(--slate)"]);
  });
  it("sorts, drops zeros, double-width pressed #1, caps at 12", () => {
    const L = layout(rows);
    expect(L.keys.map((k) => k.label)).toEqual(["cd", "git", "ls"]);
    expect(L.keys[0].slots).toBe(2);
    expect(L.keys[0].w).toBeCloseTo(3.95);
    expect(L.keys[0].h).toBeCloseTo(2.4 - 0.25);
    expect(L.keys.map((k) => k.slot)).toEqual([0, 2, 3]);
    const many = layout(Array.from({ length: 20 }, (_, i) => ({ label: `c${i}`, value: 100 - i })));
    expect(many.keys).toHaveLength(12);
    expect(many.keys.map((k) => k.row)).toEqual([0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3]);
    expect(layout([]).keys).toEqual([]);
  });
});

describe("keyTip", () => {
  const L = layout(rows);
  it("spacebar tip", () => {
    const t = keyTip(L.keys[0], L.keys, 1000, 20);
    expect(t.title).toBe("cd");
    expect(t.sub).toBe("the spacebar of your terminal");
    expect(t.rows).toEqual([
      ["Runs", "401"],
      ["Share of shell commands", "40%"],
      ["Rank", "#1 of 3"],
      ["Per worked day", "20.1 runs"],
    ]);
    expect(t.note).toBe("More than all 2 other keys combined.");
    expect(t.bar).toEqual({ value: 401, max: 401, label: "100% of cd" });
  });
  it("lower key compares to the top", () => {
    const t = keyTip(L.keys[1], L.keys, 1000);
    expect(t.note).toBe("2.7× less pressed than cd.");
    expect(t.rows).toHaveLength(3);
    expect(t.bar?.label).toBe("37% of cd");
  });
  it("edge cases: single key, tiny share, ratio note", () => {
    const one = layout([{ label: "ls", value: 1 }]);
    expect(keyTip(one.keys[0], one.keys, 1_000_000).note).toBe("The only key you pressed.");
    expect(keyTip(one.keys[0], one.keys, 1_000_000).rows?.[1][1]).toBe("<1%");
    const two = layout([{ label: "a", value: 10 }, { label: "b", value: 8 }, { label: "c", value: 7 }]);
    expect(keyTip(two.keys[0], two.keys, 25).note).toBe("1.3× as often as b.");
    const pair = layout([{ label: "a", value: 10 }, { label: "b", value: 8 }]);
    expect(keyTip(pair.keys[0], pair.keys, 18).note).toBe("More than the other key.");
    const big = layout([{ label: "a", value: 12_345_678 }]);
    expect(keyTip(big.keys[0], big.keys, 12_345_678).rows?.[0][1]).toBe("12,345,678");
  });
});

describe("ShellKeycaps render", () => {
  it("headline, data-stip on every key, no <title>", () => {
    const { container, getByText } = render(<ShellKeycaps rows={rows} total={1000} days={20} />);
    expect(getByText(/cd is your most-pressed key: 401 times/)).toBeTruthy();
    const tipped = container.querySelectorAll("[data-stip]");
    expect(tipped).toHaveLength(3);
    expect(parseTip(tipped[0].getAttribute("data-stip"))?.title).toBeTruthy();
    expect(tipped[0].getAttribute("tabindex")).toBe("0");
    expect(tipped[0].getAttribute("aria-label")).toMatch(/runs$/);
    expect(container.querySelectorAll("title")).toHaveLength(0);
    expect(container.querySelector("svg")?.getAttribute("role")).toBe("img");
  });
  it("empty state", () => {
    const { getByText, container } = render(<ShellKeycaps rows={[]} />);
    expect(getByText("The keyboard is unplugged")).toBeTruthy();
    expect(container.querySelectorAll("[data-stip]")).toHaveLength(0);
  });

  it("ties and near-ties never claim 'less pressed'", () => {
    const t = layout([{ label: "a", value: 5 }, { label: "b", value: 5 }, { label: "c", value: 1 }]);
    expect(keyTip(t.keys[0], t.keys, 11).note).toBe("Tied with b for most-pressed.");
    expect(keyTip(t.keys[1], t.keys, 11).note).toBe("Tied with a for most-pressed.");
    expect(keyTip(t.keys[2], t.keys, 11).note).toBe("5.0× less pressed than a.");
    const n = layout([{ label: "a", value: 100 }, { label: "b", value: 99 }]);
    expect(keyTip(n.keys[1], n.keys, 199).note).toBe("Neck and neck with a.");
  });

  it("rank denominator uses the caller's count", () => {
    const L = layout([{ label: "a", value: 2 }, { label: "b", value: 1 }]);
    expect(keyTip(L.keys[1], L.keys, 3, 0, 40).rows?.[2][1]).toBe("#2 of 40");
  });
});
