import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { ProjectPodium, confetti, layout, podiumTip, projectName, runwayHeight } from "./ProjectPodium";
import { parseTip } from "./tip";

afterEach(cleanup);

const H = 3600;
const rows = [
  { label: "vitinn-infra", value: 63 * H + 19 * 60 },
  { label: "worklog", value: 20 * H },
  { label: "(no folder)", value: 19 * H },
  { label: "a", value: 10 * H },
  { label: "b", value: 5 * H },
  { label: "zero", value: 0 },
];

describe("ProjectPodium helpers", () => {
  it("names the no-folder bucket", () => {
    expect(projectName("(no folder)")).toBe("no project");
    expect(projectName("x")).toBe("x");
  });

  it("scales runway height to hours with a floor", () => {
    expect(runwayHeight(10, 10)).toBe(2.2);
    expect(runwayHeight(5, 10)).toBeCloseTo(1.1);
    expect(runwayHeight(0.001, 10)).toBe(0.3);
    expect(runwayHeight(5, 0)).toBe(0.3);
  });

  it("lays out podium by rank and runway 4..", () => {
    const L = layout(rows);
    expect(L.rows.length).toBe(5);
    expect(L.steps.map((s) => s.rank)).toEqual([2, 1, 3]);
    expect(L.steps[1].h).toBeGreaterThan(L.steps[0].h);
    expect(L.runway.map((r) => r.rank)).toEqual([4, 5]);
    expect(layout([{ label: "x", value: 5 }]).steps.map((s) => s.rank)).toEqual([1]);
    expect(layout([{ label: "x", value: 0 }]).rows).toEqual([]);
  });

  it("confetti is deterministic", () => {
    expect(confetti(14)).toEqual(confetti(14));
    expect(confetti(5).length).toBe(5);
  });

  it("builds a winner tip with exact strings", () => {
    const t = podiumTip(layout(rows).rows, 1, undefined, 19);
    expect(t.title).toBe("vitinn-infra");
    expect(t.sub).toBe("gold · #1 of 5 projects");
    expect(t.rows).toContainEqual(["Work time", "63h 19m"]);
    expect(t.rows).toContainEqual(["Per worked day", "3h 20m"]);
    expect(t.rows).toContainEqual(["Lead over #2", "+43h 19m"]);
    expect(t.rows).toContainEqual(["Share of work", "54%"]);
    expect(t.note).toBe("More than #2 and #3 combined.");
  });

  it("builds a chaser tip, close gap and big gap", () => {
    const r = layout(rows).rows;
    const t3 = podiumTip(r, 3, undefined, undefined);
    expect(t3.title).toBe("no project");
    expect(t3.rows).toContainEqual(["Gap to #2", "−1h"]);
    expect(t3.rows?.some(([k]) => k === "Per worked day")).toBe(false);
    expect(t3.note).toBe("Only 1h behind #2: one good afternoon swaps them.");
    expect(podiumTip(r, 4).note).toBe("#3 got 1.9× the time.");
  });

  it("handles a single project and zero total", () => {
    const t = podiumTip([{ label: "solo", value: 100 }], 1, 0);
    expect(t.note).toBe("The only project on the board.");
    expect(t.rows).toContainEqual(["Share of work", "100%"]);
    expect(t.rows?.some(([k]) => k.startsWith("Lead"))).toBe(false);
  });
});

describe("ProjectPodium render", () => {
  it("tips every mark and leaves no native <title>", () => {
    const { container } = render(<ProjectPodium rows={rows} days={19} />);
    const marks = container.querySelectorAll("[data-stip]");
    expect(marks.length).toBe(5);
    expect(parseTip(marks[0].getAttribute("data-stip"))?.title).toBe("worklog");
    expect(marks[0].getAttribute("tabindex")).toBe("0");
    expect(marks[0].getAttribute("aria-label")).toContain("#2");
    expect(container.querySelector("title")).toBeNull();
    expect(container.querySelector("svg")?.getAttribute("role")).toBe("img");
    expect(container.textContent).toContain("vitinn-infra takes gold with 63h 19m");
    expect(container.textContent).toContain("no project");
  });

  it("shows an empty state", () => {
    const { container } = render(<ProjectPodium rows={[]} />);
    expect(container.textContent).toContain("The podium is empty");
    expect(container.querySelector("[data-stip]")).toBeNull();
  });
});

describe("podiumTip honesty and ties", () => {
  const mk = (vs: number[]) => vs.map((v, i) => ({ label: `p${i}`, value: v * 3600 }));
  it("no project count when the list is truncated", () => {
    const t = podiumTip(mk([20, 19, 18, 17, 16, 15, 14, 13, 12, 11]), 4, 500 * 3600);
    expect(t.sub).toBe("#4");
    expect(t.rows).toContainEqual(["Rank", "#4"]);
  });
  it("ties and near-ties", () => {
    const eq = mk([1, 1, 1]);
    expect(podiumTip(eq, 1).note).toBe("Tied with #2.");
    expect(podiumTip(eq, 2).note).toBe("Tied with #1.");
    expect(podiumTip([{ label: "a", value: 3700 }, { label: "b", value: 3600 }], 1).note).toBe("Leads #2 by 2m.");
  });
});
