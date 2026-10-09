import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { DailyStack, scale } from "./DailyStack";
import { day } from "./time-fixtures";

afterEach(cleanup);

describe("scale", () => {
  it("always reaches the target and rounds up", () => {
    expect(scale([day({ day: "2026-10-06", work_seconds: 3600 })]).top).toBe(8);
    expect(scale([day({ day: "2026-10-06", work_seconds: 11 * 3600 + 1 })]).top).toBe(12);
  });
  it("keeps a tick step of at least one hour", () => expect(scale([]).step).toBe(2));
});

describe("DailyStack", () => {
  it("renders aria numbers and counts days over target", () => {
    const daily = [
      day({ day: "2026-10-06", work_seconds: 9 * 3600, personal_seconds: 1800 }),
      day({ day: "2026-10-07", work_seconds: 3600, ignored_seconds: 600 }),
    ];
    const { container } = render(<DailyStack daily={daily} />);
    const label = container.querySelector("svg")!.getAttribute("aria-label")!;
    expect(label).toContain("10h work");
    expect(label).toContain("1 days above 7.5");
    expect(container.innerHTML).not.toContain("NaN");
  });
  it("empty state", () => {
    const { container } = render(<DailyStack daily={[day({ day: "2026-10-06" })]} />);
    expect(container.textContent).toContain("No data yet");
  });
  it("handles huge values", () => {
    const { container } = render(<DailyStack daily={[day({ day: "2026-10-06", work_seconds: 1e9 })]} />);
    expect(container.innerHTML).not.toContain("NaN");
    expect(container.innerHTML).not.toContain("Infinity");
  });
});
