import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import type { LooseEnd, LooseEnds } from "@/lib/time_leaks_contract";
import { Unbilled } from "./Unbilled";

afterEach(cleanup);

const item = (over: Partial<LooseEnd>): LooseEnd => ({
  block_id: 1,
  day: "2026-10-05",
  jira_issue: "ABC-1",
  started_at: "2026-10-05T09:00:00Z",
  ended_at: "2026-10-05T10:30:00Z",
  seconds: 5400,
  description: null,
  missing: ["tempo", "export"],
  ...over,
});
const data = (items: LooseEnd[]): LooseEnds => ({ cycle_start: "2026-09-20", cycle_end: "2026-10-19", items });
const TODAY = "2026-10-12";

describe("Unbilled", () => {
  it("explains the daemon is down and how to start it", () => {
    // catches: swallowing the error into an empty, all-clear page
    render(<Unbilled data={null} today={TODAY} hourNow={9} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("Couldn’t reach the worklog daemon");
    expect(alert.textContent).toContain("worklog daemon install");
    expect(screen.queryByText(/Nothing left/)).toBeNull();
  });

  it("says the cycle is settled when nothing is open", () => {
    // catches: a blank page or an error for an empty cycle
    render(<Unbilled data={data([])} today={TODAY} hourNow={9} />);
    expect(screen.getByRole("heading", { name: "Nothing left to finish" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByRole("heading", { name: /Days to finish/ })).toBeNull();
  });

  it("shows one row per day with the right fix for each step", () => {
    // catches: a row per block, or a wrong link for either step
    render(
      <Unbilled
        data={data([item({}), item({ block_id: 2, jira_issue: "XYZ-9", seconds: 1800, missing: ["export"] })])}
        today={TODAY}
        hourNow={9}
      />,
    );
    const row = document.getElementById("day-2026-10-05")!;
    expect(document.querySelectorAll(".ub-row")).toHaveLength(1);
    expect(row.dataset.state).toBe("tempo");
    expect(row.textContent).toContain("Mon 5 Oct");
    expect(row.textContent).toContain("ABC-1");
    expect(row.textContent).not.toContain("XYZ-9"); // XYZ-9 is already in Tempo
    expect(row.querySelector(".ub-row-hours")!.textContent).toBe("1h 30m"); // the Tempo-missing hours
    // Tempo comes first: a day not in Tempo offers only the Tempo fix
    const links = within(row).getAllByRole("link");
    expect(links.map((a) => [a.textContent, a.getAttribute("href")])).toEqual([["Fix Tempo line", "/2026-10-05"]]);
  });

  it("today's unsent work waits for the 17:00 send and has no fix button", () => {
    // catches: telling the Owner to fix work the 17:00 send will handle
    render(<Unbilled data={data([item({ day: TODAY, missing: ["tempo"] })])} today={TODAY} hourNow={10} />);
    const card = document.getElementById(`day-${TODAY}`)!;
    expect(card.textContent).toContain("Sends to Tempo at 17:00 today");
    expect(within(card).queryByRole("link", { name: "Fix Tempo line" })).toBeNull();
  });

  it("the cycle strip names each day's state for screen readers and links open days", () => {
    // catches: a colour-only strip, or cells that do not jump to their card
    render(<Unbilled data={data([item({ missing: ["export"] })])} today={TODAY} hourNow={9} />);
    expect(screen.getByLabelText("Mon 5 Oct: in Tempo, not marked billed").getAttribute("href")).toBe("#day-2026-10-05");
    expect(screen.getByLabelText(/^Sun 20 Sept?: no open work$/).tagName).toBe("SPAN");
    expect(screen.getByLabelText("Tue 13 Oct: not yet").tagName).toBe("SPAN");
  });

  it("days with only billing left sit in their own group with the billing fix only", () => {
    // catches: mixing billing-only days into the Tempo group, or offering a Tempo fix for them
    render(<Unbilled data={data([item({ missing: ["export"] }), item({ block_id: 2, day: "2026-10-06", missing: ["tempo", "export"] })])} today={TODAY} hourNow={9} />);
    const group = screen.getByRole("heading", { name: /In Tempo, not marked billed/ }).closest("section")!;
    const row = group.querySelector(".ub-row") as HTMLElement;
    expect(row.id).toBe("day-2026-10-05");
    expect(row.dataset.state).toBe("bill");
    expect(row.textContent).toContain("1h 30m");
    expect(within(row).getAllByRole("link").map((a) => a.getAttribute("href"))).toEqual(["/billing?from=2026-10-05"]);
    expect(group.textContent).toContain("Mark exported");
  });

  it("the step summary counts hours and days per step", () => {
    // catches: summing a both-missing block into one total twice
    render(<Unbilled data={data([item({}), item({ block_id: 2, day: "2026-10-06", missing: ["export"], seconds: 1800 })])} today={TODAY} hourNow={9} />);
    const steps = screen.getByRole("list", { name: "How a day gets finished" });
    expect(steps.textContent).toContain("1h 30m not sent · 1 day");
    expect(steps.textContent).toContain("2h not marked · 2 days");
    expect(steps.textContent).toContain("2h still open this cycle");
  });
});
