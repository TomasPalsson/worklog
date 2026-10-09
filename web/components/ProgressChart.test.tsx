import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { ProgressChart } from "./ProgressChart";
import type { TicketProgress } from "@/lib/types";

afterEach(cleanup);

// 2026-10-06 is a Tuesday. You 30m/1h/30m, Jón Geir 1h/30m/1h 30m by day.
function ticket(over: Partial<TicketProgress> = {}): TicketProgress {
  return {
    key: "GENAI-1897",
    estimate_seconds: 14400,
    people: [
      { account_id: "me", name: "Tomas P", is_you: true, seconds: 7200, by_day: [["2026-10-06", 1800], ["2026-10-07", 3600], ["2026-10-08", 1800]] },
      { account_id: "jg", name: "Jón Geir", is_you: false, seconds: 9000, by_day: [["2026-10-06", 3600], ["2026-10-07", 1800], ["2026-10-08", 3600]] },
    ],
    logged_seconds: 16200,
    pulled_at: "2026-10-09T09:42:00Z",
    error: null,
    ...over,
  };
}

function rows(): string[][] {
  const table = screen.getByRole("table", { hidden: true });
  return within(table)
    .getAllByRole("row", { hidden: true })
    .map((r) => [...r.querySelectorAll("th,td")].map((c) => c.textContent ?? ""));
}

describe("ProgressChart", () => {
  it("offers Show as table with cumulative per-person totals (a per-day, non-cumulative table fails)", () => {
    render(<ProgressChart ticket={ticket()} pending={0} />);
    expect(screen.getByText("Show as table")).toBeTruthy();
    expect(rows()).toEqual([
      ["Total by end of", "You", "Jón Geir", "Total"],
      ["Tue 6", "30m", "1h", "1h 30m"],
      ["Wed 7", "1h 30m", "1h 30m", "3h"],
      ["Thu 8", "2h", "2h 30m", "4h 30m"],
    ]);
  });

  it("adds an After sync row with the pending hours on You (omitting it, or adding it to a teammate, fails)", () => {
    render(<ProgressChart ticket={ticket()} pending={3600} />);
    expect(rows().at(-1)).toEqual(["After sync", "3h", "2h 30m", "5h 30m"]);
  });

  it("has no After sync row when nothing is pending (always adding the row fails)", () => {
    render(<ProgressChart ticket={ticket()} pending={0} />);
    expect(screen.queryByText("After sync")).toBeNull();
  });

  it("labels the chart with the days and the estimate", () => {
    render(<ProgressChart ticket={ticket()} pending={0} />);
    const label = screen.getByRole("group", { name: /Running total of hours on GENAI-1897/ }).getAttribute("aria-label");
    expect(label).toBe("Running total of hours on GENAI-1897: Tue 6 1h 30m, Wed 7 3h, Thu 8 4h 30m; estimate 4h");
  });

  it("draws the dashed estimate line and an amber band only when the total passes it", () => {
    const { container, rerender } = render(<ProgressChart ticket={ticket()} pending={0} />);
    expect(container.querySelector(".ep-est-line")).not.toBeNull();
    expect(container.querySelector(".ep-over-band")).not.toBeNull();
    // total exactly at the estimate is not over (>= instead of > fails)
    rerender(<ProgressChart ticket={ticket({ estimate_seconds: 16200 })} pending={0} />);
    expect(container.querySelector(".ep-over-band")).toBeNull();
  });

  it("draws the pending dot only with pending hours", () => {
    const { container, rerender } = render(<ProgressChart ticket={ticket()} pending={0} />);
    expect(container.querySelector(".ep-pend-dot")).toBeNull();
    rerender(<ProgressChart ticket={ticket()} pending={3600} />);
    expect(container.querySelector(".ep-pend-dot")).not.toBeNull();
  });

  it("renders nothing when no day was logged", () => {
    const { container } = render(<ProgressChart ticket={ticket({ people: [], logged_seconds: 0 })} pending={3600} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing without an estimate", () => {
    const { container } = render(<ProgressChart ticket={ticket({ estimate_seconds: null })} pending={0} />);
    expect(container.firstChild).toBeNull();
  });

  it("arrow keys walk the days and clamp at both ends; the tooltip shows that day's totals", () => {
    render(<ProgressChart ticket={ticket()} pending={3600} />);
    const svg = screen.getByRole("group", { name: /Running total/ });
    expect(screen.queryByText(/Total by end of/, { selector: "b" })).toBeNull();
    fireEvent.focus(svg);
    // starts on the last day, with the pending hours and the verdict
    expect(screen.getByText("Total by end of Thu 8", { selector: "b" })).toBeTruthy();
    expect(screen.getByText("+1h")).toBeTruthy();
    expect(screen.getByText("5h 30m of 4h")).toBeTruthy();
    expect(screen.getByText("1h 30m over")).toBeTruthy();
    fireEvent.keyDown(svg, { key: "ArrowRight" });
    expect(screen.getByText("Total by end of Thu 8", { selector: "b" })).toBeTruthy();
    fireEvent.keyDown(svg, { key: "ArrowLeft" });
    expect(screen.getByText("Total by end of Wed 7", { selector: "b" })).toBeTruthy();
    // an earlier day carries no pending row and reads "left"
    expect(screen.queryByText("+1h")).toBeNull();
    expect(screen.getByText("3h of 4h")).toBeTruthy();
    expect(screen.getByText("1h left")).toBeTruthy();
    fireEvent.keyDown(svg, { key: "ArrowLeft" });
    fireEvent.keyDown(svg, { key: "ArrowLeft" });
    expect(screen.getByText("Total by end of Tue 6", { selector: "b" })).toBeTruthy();
    fireEvent.keyDown(svg, { key: "Escape" });
    expect(screen.queryByText(/Total by end of/, { selector: "b" })).toBeNull();
  });

  it("hides the tooltip on blur", () => {
    render(<ProgressChart ticket={ticket()} pending={0} />);
    const svg = screen.getByRole("group", { name: /Running total/ });
    fireEvent.focus(svg);
    fireEvent.blur(svg);
    expect(screen.queryByText(/Total by end of/, { selector: "b" })).toBeNull();
  });
});
