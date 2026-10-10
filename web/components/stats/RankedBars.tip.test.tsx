import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { RankedBars, rowTip } from "./RankedBars";

afterEach(cleanup);

describe("rowTip", () => {
  const rows = [{ label: "Bash", value: 360 }, { label: "Read", value: 120 }, { label: "Edit", value: 120 }, { label: "Write", value: 0 }];
  it("tied leaders are not 'in front'", () => {
    const r = [{ label: "A", value: 30 }, { label: "B", value: 30 }, { label: "C", value: 10 }];
    const t = rowTip(r, 0, "count");
    expect(t.sub).toBe("joint top of the list");
    expect(t.note).toBe("Tied for 1st with 1 other.");
    expect(t.rows!.some(([l]) => l === "Lead over next")).toBe(false);
    expect(rowTip([{ label: "A", value: 9 }, { label: "B", value: 9 }], 0, "count").note).toBe("Tied for 1st with 1 other.");
    expect(rowTip([{ label: "A", value: 9 }], 0, "count").note).toBe("The only one on the list.");
  });
  it("leader", () => {
    const t = rowTip(rows, 0, "count");
    expect(t.title).toBe("Bash");
    expect(t.sub).toBe("top of the list");
    expect(t.rows).toEqual([["Value", "360"], ["Share of list", "60%"], ["Rank", "1 of 4"], ["Lead over next", "240"]]);
    expect(t.note).toBe("3× the runner-up.");
    expect(t.bar).toEqual({ value: 360, max: 360, label: "100% of the leader" });
  });
  it("ties share a rank; trailing rows compare to the leader", () => {
    const t = rowTip(rows, 2, "count");
    expect(t.sub).toBe("2nd place");
    expect(t.rows).toContainEqual(["Behind the leader", "240"]);
    expect(t.note).toBe("33% of the leader's 360.");
  });
  it("zero row, durations, full label and lone rows", () => {
    expect(rowTip(rows, 3, "count").note).toBe("Nothing here yet.");
    expect(rowTip([{ label: "a", value: 5400 }, { label: "b", value: 1800 }], 0, "seconds", "mcp__server__tool").title).toBe("mcp__server__tool");
    expect(rowTip([{ label: "only", value: 9 }], 0, "count").note).toBe("The only one on the list.");
    expect(rowTip([{ label: "a", value: 5 }, { label: "b", value: 0 }], 0, "count").note).toBe("The only one with any activity.");
  });
});

describe("RankedBars tips", () => {
  it("tips each row with the untruncated label and no native title", () => {
    const { container } = render(
      <RankedBars title="Tools" unit="count" shorten rows={[{ label: "mcp__github__create_pull_request", value: 10 }, { label: "Read", value: 5 }]} />,
    );
    const li = container.querySelectorAll("li[data-stip]");
    expect(li.length).toBe(2);
    expect(JSON.parse(li[0].getAttribute("data-stip")!).title).toBe("mcp__github__create_pull_request");
    expect(li[0].getAttribute("tabindex")).toBe("0");
    expect(container.querySelectorAll("[title]").length).toBe(0);
  });
});
