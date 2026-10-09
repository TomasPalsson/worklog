import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { BillableDays, TicketRing, daySplit } from "./BillableDays";
import type { BillingClass, MirresDay, TempoLine } from "@/lib/tempo_line_contract";

afterEach(cleanup);

const line = (secs: number, cls: BillingClass | null): TempoLine => ({
  day: "2026-10-06",
  jira_issue: "X-1",
  text: null,
  text_origin: null,
  fallback_text: "",
  union_seconds: secs,
  hours_override_seconds: null,
  effective_seconds: secs,
  billing: cls && {
    account_key: "A",
    project: null,
    project_type: null,
    class: cls,
    warning: null,
    customer: null,
    details: null,
  },
});
const day = (d: string, lines: TempoLine[]): MirresDay => ({ day: d, fetched_at: "2026-10-07T00:00:00Z", lines });

describe("daySplit", () => {
  it("splits by class, unmatched lines are missing", () => {
    const s = daySplit(day("2026-10-06", [line(3600, "billable"), line(1800, "included"), line(1800, null)]));
    expect(s.map((x) => x.kind)).toEqual(["billable", "included", "missing"]);
    expect(s.map((x) => Math.round(x.share * 100))).toEqual([50, 25, 25]);
  });

  it("is empty for a day with no hours", () => {
    expect(daySplit(day("2026-10-06", []))).toEqual([]);
  });
});

describe("BillableDays", () => {
  it("one linked column per day, labelled, with the 70% target", () => {
    const { container } = render(
      <BillableDays days={[day("2026-10-06", [line(3600, "billable")]), day("2026-10-07", [line(3600, "not_billable")])]} />,
    );
    expect(container.querySelectorAll("a").length).toBe(2);
    expect(container.querySelector("a")?.getAttribute("href")).toBe("#mirres-day-2026-10-06");
    expect(container.textContent).toContain("70%");
    const label = screen.getByRole("img").getAttribute("aria-label") ?? "";
    expect(label).toContain("2026-10-06 100%");
    expect(label).toContain("2026-10-07 0%");
  });

  it("says no Mirres data when no line is matched", () => {
    render(<BillableDays days={[day("2026-10-08", [line(3600, null)])]} />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("2026-10-08 no Mirres data");
  });

  it("renders nothing without days", () => {
    expect(render(<BillableDays days={[]} />).container.innerHTML).toBe("");
  });
});

describe("TicketRing", () => {
  const frac = (m: number, t: number) =>
    render(<TicketRing matched={m} total={t} />).container.querySelector("svg")?.getAttribute("data-fraction");

  it("sets the arc length inline, where .art-draw's dasharray can't override it", () => {
    const arc = render(<TicketRing matched={1} total={3} />).container.querySelector(".art-billing-ring-arc") as SVGElement;
    expect(arc.style.strokeDasharray).toMatch(/^0\.333/);
  });

  it("is matched / total, clamped, zero-safe", () => {
    expect(frac(1, 4)).toBe("0.25");
    expect(frac(5, 4)).toBe("1");
  });

  it("draws nothing when there are no tickets", () => {
    expect(render(<TicketRing matched={0} total={0} />).container.innerHTML).toBe("");
  });

  it("is aria-hidden", () => {
    const { container } = render(<TicketRing matched={1} total={2} />);
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });
});
