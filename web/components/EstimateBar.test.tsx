import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { EstimateBar } from "./EstimateBar";
import { formatClock } from "@/lib/format";
import type { PersonHours, TicketProgress } from "@/lib/types";

afterEach(cleanup);

const H = 3600;
const person = (id: string, name: string, seconds: number, is_you = false): PersonHours => ({
  account_id: id,
  name,
  is_you,
  seconds,
  by_day: [["2026-10-08", seconds]],
});

function ticket(over: Partial<TicketProgress> = {}): TicketProgress {
  return {
    key: "GENAI-1897",
    estimate_seconds: 4 * H,
    people: [person("me", "Tomas P", 2 * H, true), person("jg", "Jón Geir", 2.5 * H)],
    logged_seconds: 4.5 * H,
    pulled_at: "2026-10-09T09:42:00Z",
    error: null,
    ...over,
  };
}

const bar = (t: TicketProgress, pending = 0) => <EstimateBar ticket={t} pending={pending} onRetry={() => {}} />;
const solo = (secs: number) => ticket({ people: [person("me", "Tomas P", secs, true)], logged_seconds: secs });

describe("EstimateBar ready", () => {
  it("shows the approved over state: head, once synced, words, legend, meter, flag", () => {
    const { container } = render(bar(ticket(), 1 * H));
    expect(screen.getByText("5h 30m", { selector: "b" })).toBeTruthy();
    expect(screen.getByText(/of 4h/)).toBeTruthy();
    expect(screen.getByText("once synced")).toBeTruthy();
    expect(screen.getByText("1h 30m over")).toBeTruthy();
    const legend = within(container.querySelector(".ep-legend") as HTMLElement);
    expect(legend.getByText("Tomas P")).toBeTruthy();
    expect(legend.getByText("Jón Geir")).toBeTruthy();
    expect(legend.getByText("TP")).toBeTruthy();
    expect(legend.getByText("JG")).toBeTruthy();
    expect(legend.getByText("2h", { selector: "b" })).toBeTruthy();
    expect(legend.getByText("2h 30m", { selector: "b" })).toBeTruthy();
    expect(legend.getByText("This block, not in Tempo yet")).toBeTruthy();
    expect(legend.getByText("+1h", { selector: "b" })).toBeTruthy();
    const meter = screen.getByRole("meter");
    expect(meter.getAttribute("aria-valuenow")).toBe(String(5.5 * H));
    expect(meter.getAttribute("aria-valuetext")).toBe("5h 30m of 4h estimate including this block, 1h 30m over");
    expect(container.querySelector(".ep-flag-l")?.textContent).toBe("4h");
    expect(container.querySelector(".ep-overzone")).not.toBeNull();
    expect(container.querySelector(".ep-fill > i.ep-pend")).not.toBeNull();
  });

  it("sizes segments against max(estimate, used) and puts the flag at the estimate", () => {
    const { container } = render(bar(ticket(), 1 * H));
    const segs = [...container.querySelectorAll<HTMLElement>(".ep-fill > i")].map((i) => i.style.width);
    // 2h, 2h 30m, +1h of 5h 30m
    expect(segs).toEqual(["36.36%", "45.45%", "18.18%"]);
    expect((container.querySelector(".ep-flag") as HTMLElement).style.left).toBe("72.73%");
  });

  it("without pending hours: no once synced, no pending piece or legend line", () => {
    const { container } = render(bar(ticket()));
    expect(screen.queryByText("once synced")).toBeNull();
    expect(screen.queryByText(/not in Tempo yet/)).toBeNull();
    expect(container.querySelector(".ep-pend")).toBeNull();
    expect(screen.getByRole("meter").getAttribute("aria-valuetext")).toBe("4h 30m of 4h estimate, 30m over");
  });

  it("exactly at the estimate is 0m left, no over zone, flag at the end (>= for > fails)", () => {
    const { container } = render(bar(solo(4 * H)));
    expect(screen.getByText("0m left")).toBeTruthy();
    expect(container.querySelector(".ep-overzone")).toBeNull();
    expect(container.querySelector(".ep-flag")?.hasAttribute("data-end")).toBe(true);
    expect(container.querySelector(".ep")?.getAttribute("data-tone")).toBe("low");
  });

  it("the flag leaves the end only when over (always data-end fails)", () => {
    const { container } = render(bar(ticket()));
    expect(container.querySelector(".ep-flag")?.hasAttribute("data-end")).toBe(false);
  });

  it("names the tone with an icon label: on track, running low, over", () => {
    const { rerender } = render(bar(solo(1 * H)));
    expect(screen.getByRole("img", { name: "On track" })).toBeTruthy();
    expect(screen.getByText("3h left")).toBeTruthy();
    rerender(bar(solo(3.2 * H))); // exactly 80%
    expect(screen.getByRole("img", { name: "Running low" })).toBeTruthy();
    rerender(bar(solo(4.5 * H)));
    expect(screen.getByRole("img", { name: "Over" })).toBeTruthy();
  });

  it("folds a 6-person ticket into 4 named + Others in bar and legend", () => {
    const people = [
      person("me", "Tomas P", 1 * H, true),
      person("a", "Ann A", 5 * H),
      person("b", "Bob B", 4 * H),
      person("c", "Cy C", 3 * H),
      person("d", "Di D", 2 * H),
      person("e", "Ed E", 1 * H),
    ];
    const { container } = render(bar(ticket({ people, logged_seconds: 16 * H, estimate_seconds: 20 * H })));
    expect(container.querySelectorAll(".ep-fill > i").length).toBe(5);
    const legend = within(container.querySelector(".ep-legend") as HTMLElement);
    expect(legend.getByText("Others")).toBeTruthy();
    expect(legend.queryByText("Di D")).toBeNull();
  });

  it("shows when Jira numbers were pulled", () => {
    render(bar(ticket()));
    expect(screen.getByText(`Jira numbers from ${formatClock("2026-10-09T09:42:00Z")}`)).toBeTruthy();
  });

  it("includes the running-total chart", () => {
    render(bar(ticket()));
    expect(screen.getByText("Show as table")).toBeTruthy();
    expect(screen.getByText("Running total on this ticket")).toBeTruthy();
  });
});

describe("EstimateBar no estimate (B7, FR-14)", () => {
  for (const estimate_seconds of [null, 0]) {
    it(`estimate ${estimate_seconds}: only the muted Jira line`, () => {
      const { container } = render(bar(ticket({ estimate_seconds })));
      expect(container.textContent).toBe(
        "No estimate on this ticket yet. Set “Original estimate” on GENAI-1897 in Jira and the bar shows up here.",
      );
      expect(screen.queryByRole("meter")).toBeNull();
      expect(screen.queryByText("Show as table")).toBeNull();
    });
  }
});

describe("EstimateBar loading and errors", () => {
  it("loading shows the Jira line and a placeholder bar", () => {
    const { container } = render(<EstimateBar ticket={undefined} pending={0} onRetry={() => {}} />);
    expect(screen.getByText("Loading hours from Jira…")).toBeTruthy();
    expect(container.querySelector(".ep")?.getAttribute("aria-busy")).toBe("true");
    expect(container.querySelector(".ep-bar.ep-skel")).not.toBeNull();
    expect(screen.queryByRole("meter")).toBeNull();
  });

  it("failed with nothing cached: error copy, and Try again calls onRetry once", () => {
    const onRetry = mock(() => {});
    const t = ticket({ error: "jira_unavailable", people: [], logged_seconds: 0, estimate_seconds: null, pulled_at: null });
    render(<EstimateBar ticket={t} pending={0} onRetry={onRetry} />);
    expect(screen.getByRole("alert").textContent).toBe(
      "Couldn't load hours for GENAI-1897 — Jira didn't answer. Your blocks are safe. Try again",
    );
    expect(screen.queryByRole("meter")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("failed with cached numbers: keeps the bar and its pulled time, still offers Try again", () => {
    const onRetry = mock(() => {});
    render(<EstimateBar ticket={ticket({ error: "jira_unavailable" })} pending={0} onRetry={onRetry} />);
    expect(screen.getByRole("meter")).toBeTruthy();
    expect(screen.getByText(/Jira numbers from/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("not configured: points to Settings, no Try again", () => {
    render(bar(ticket({ error: "not_configured", estimate_seconds: null, people: [], logged_seconds: 0, pulled_at: null })));
    expect(screen.getByText("Connect Jira in Settings to see estimates.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });
});
