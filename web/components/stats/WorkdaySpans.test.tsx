import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { median, spans, WorkdaySpans } from "./WorkdaySpans";
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
