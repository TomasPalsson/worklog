import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { median, medianTip, ordinal, spans, spanTip, WorkdaySpans } from "./WorkdaySpans";
import { parseTip } from "./tip";
import { day } from "./time-fixtures";

afterEach(cleanup);

describe("median", () => {
  it("handles odd, even and empty", () => {
    expect(median([3, 1, 2])).toBe(2);
    expect(median([1, 2, 3, 4])).toBe(2.5);
    expect(median([])).toBeNull();
  });
});

describe("WorkdaySpans", () => {
  const daily = [
    day({ day: "2026-10-05", first_at: "08:00", last_at: "17:00", work_seconds: 28800 }),
    day({ day: "2026-10-06", first_at: "09:00", last_at: "18:00", work_seconds: 25000 }),
    day({ day: "2026-10-07" }),
    day({ day: "2026-10-10", first_at: "05:00", last_at: "23:59", work_seconds: 100 }),
  ];
  it("skips null days", () => expect(spans(daily).length).toBe(3));
  it("states median start and stop", () => {
    const { container } = render(<WorkdaySpans daily={daily} />);
    expect(container.querySelector("p")!.textContent).toContain("start at 08:00 and stop at 18:00");
    expect(container.querySelector("svg")!.getAttribute("aria-label")).toContain("Median start 08:00");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("empty state when no spans", () => {
    const { container } = render(<WorkdaySpans daily={[day({ day: "2026-10-05" })]} />);
    expect(container.textContent).toContain("No data yet");
  });
  it("single day works", () => {
    const { container } = render(<WorkdaySpans daily={[daily[0]]} />);
    expect(container.innerHTML).not.toContain("NaN");
  });
});

describe("span tips", () => {
  const daily = [
    day({ day: "2026-10-05", first_at: "08:00", last_at: "17:00", work_seconds: 28800, prompts: 1200 }),
    day({ day: "2026-10-06", first_at: "09:12", last_at: "18:00", work_seconds: 25000, prompts: 3 }),
    day({ day: "2026-10-10", first_at: "10:00", last_at: "10:00", work_seconds: 0 }),
  ];
  const list = spans(daily);
  it("ordinal", () => {
    expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 112].map(ordinal)).toEqual(["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "112th"]);
  });
  it("candle tip", () => {
    const t = spanTip(list[1], list, 3);
    expect(t.title).toBe("Tue 6 Oct");
    expect(t.sub).toBe("2nd longest of 3 spans");
    expect(t.rows).toEqual([
      ["Started", "09:12"],
      ["Finished", "18:00"],
      ["Span length", "8h 48m"],
      ["Work inside it", "6h 57m"],
      ["Prompts", "3"],
      ["Start rank", "2nd earliest of 3"],
    ]);
    expect(t.bar).toEqual({ value: 528, max: 540, label: "8h 48m of the longest span, 9h" });
    expect(t.note).toBe("Right on your usual start (median 09:12).");
    expect(spanTip(list[0], list, 1200).note).toBe("Started 1h 12m earlier than usual (median 09:12).");
  });
  it("zero-length weekend candle, huge prompts, single day", () => {
    const w = spanTip(list[2], list, 1_000_000);
    expect(w.sub).toBe("3rd longest of 3 spans · weekend");
    expect(w.rows![2]).toEqual(["Span length", "0m"]);
    expect(w.note).toBe("Started 48m later than usual (median 09:12).");
    expect(spanTip(list[0], [list[0]], 1200).note).toContain("Only one tracked day");
    expect(spanTip(list[0], list, 1200).rows![4]).toEqual(["Prompts", "1,200"]);
  });
  it("median tip", () => {
    const t = medianTip("start", list);
    expect(t.title).toBe("Median start");
    expect(t.rows).toEqual([
      ["Median", "09:12"],
      ["Earliest start", "08:00"],
      ["Latest start", "10:00"],
      ["Spread", "2h"],
      ["Days at or before it", "2 of 3"],
    ]);
    expect(medianTip("finish", [list[2]]).note).toBe("Every finish landed on the same minute.");
  });
  it("renders tips and drops native titles", () => {
    const { container } = render(<WorkdaySpans daily={daily} />);
    const tipped = container.querySelectorAll("[data-stip]");
    expect(tipped.length).toBe(5);
    expect(parseTip(tipped[0].getAttribute("data-stip"))!.title).toBe("Mon 5 Oct");
    expect(container.querySelectorAll("svg title").length).toBe(0);
    expect(tipped[0].getAttribute("aria-label")).toContain("08:00 to 17:00");
  });
});
