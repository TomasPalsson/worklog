import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { ActivityStream, buildLayers, columnTip, layerTip } from "./ActivityStream";
import { day } from "./time-fixtures";

afterEach(cleanup);

describe("activity tips", () => {
  const daily = [
    day({ day: "2026-10-05", prompts: 10, commits: 1 }),
    day({ day: "2026-10-06", prompts: 40 }),
    day({ day: "2026-10-07", prompts: 10 }),
  ];
  it("layerTip", () => {
    const layers = buildLayers(daily);
    const t = layerTip(layers[0], layers, 3);
    expect(t.title).toBe("prompts");
    expect(t.sub).toBe("1st biggest of 2 streams");
    expect(t.rows).toEqual([
      ["Total", "60"],
      ["Peak day", "40 on 6 Oct"],
      ["Active days", "3 of 3"],
      ["Avg per active day", "20"],
      ["Share of all counts", "98%"],
    ]);
    expect(t.note).toBe("Peak day was 2× its average active day.");
    expect(layerTip(layers[1], layers, 3).note).toBe("Steady: no day stood far above the rest.");
  });
  it("columnTip lists every series and the peaks", () => {
    const t = columnTip(daily[1], daily);
    expect(t.title).toBe("Tue 6 Oct");
    expect(t.rows!.length).toBe(6);
    expect(t.rows![0]).toEqual(["prompts", "40"]);
    expect(t.rows![5]).toEqual(["commits", "0"]);
    expect(t.note).toBe("Peak day for prompts.");
    expect(columnTip(daily[0], daily).note).toBe("Peak day for commits.");
    expect(columnTip(daily[2], daily).note).toBe("No stream peaked this day.");
  });
  it("renders a column per day and tipped layers without native titles", () => {
    const { container } = render(<ActivityStream daily={daily} />);
    expect(container.querySelectorAll("title").length).toBe(0);
    expect(container.querySelectorAll("rect.st-col[data-stip]").length).toBe(3);
    expect(container.querySelectorAll("path.st-layer[data-stip]").length).toBe(2);
    expect(container.querySelectorAll("li[data-stip]").length).toBe(2);
  });
  it("single day still gets one column", () => {
    const { container } = render(<ActivityStream daily={[day({ day: "2026-10-05", prompts: 5 })]} />);
    expect(container.querySelectorAll("rect.st-col").length).toBe(1);
    expect(container.innerHTML).not.toContain("NaN");
  });
});
