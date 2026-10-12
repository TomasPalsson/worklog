import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import type { TempoGap, UnbilledDay, UnbilledDays } from "@/lib/time_leaks_contract";
import { Unbilled } from "./Unbilled";

afterEach(cleanup);

const gap = (over: Partial<TempoGap>): TempoGap => ({
  jira_issue: "ABC-1",
  seconds: 5400,
  in_tempo_seconds: 0,
  reason: "not_sent",
  ...over,
});
const day = (over: Partial<UnbilledDay>): UnbilledDay => ({
  day: "2026-10-05",
  dismissed: null,
  tempo: [],
  ticketless_seconds: 0,
  export: null,
  ...over,
});
const data = (days: UnbilledDay[], tempo_pulled_at: string | null = "2026-10-12T07:00:00Z"): UnbilledDays => ({
  cycle_start: "2026-09-20",
  cycle_end: "2026-10-19",
  tempo_pulled_at,
  days,
});
const TODAY = "2026-10-12";
const NOW = new Date("2026-10-12T09:00:00Z");
const page = (d: UnbilledDays | null, hourNow = 9) => render(<Unbilled data={d} today={TODAY} hourNow={hourNow} now={NOW} />);
const section = (name: RegExp) => screen.getByRole("heading", { name }).closest("section")!;

describe("Unbilled", () => {
  it("explains the daemon is down and how to start it", () => {
    // catches: swallowing the error into an empty, all-clear page
    page(null);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("Couldn’t reach the worklog daemon");
    expect(alert.textContent).toContain("worklog daemon install");
    expect(screen.queryByText(/Nothing left/)).toBeNull();
  });

  it("says the cycle is settled when nothing is open", () => {
    // catches: a blank page or an error for an empty cycle
    page(data([]));
    expect(screen.getByRole("heading", { name: "Nothing left to finish" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByRole("heading", { name: /Not in Tempo yet/ })).toBeNull();
  });

  it("a Tempo row lists each ticket's missing hours with why, and links to the day", () => {
    // catches: showing the full line for a short ticket, a missing reason word, or a wrong link
    page(
      data([
        day({
          tempo: [
            gap({ jira_issue: "A-1", seconds: 3600, in_tempo_seconds: 1800, reason: "short" }),
            gap({ jira_issue: "B-2", seconds: 4920, in_tempo_seconds: 3600, reason: "deleted" }),
            gap({ jira_issue: "C-3", seconds: 600 }),
          ],
          export: { seconds: 9000 },
        }),
      ]),
    );
    const row = section(/Not in Tempo yet/).querySelector(".ub-row") as HTMLElement;
    expect(row.id).toBe("day-2026-10-05");
    expect(row.dataset.state).toBe("tempo");
    const chips = [...row.querySelectorAll(".ub-tickets li")].map((li) => li.textContent);
    expect(chips).toEqual(["A-1 30m · only 30m in Tempo", "B-2 22m · deleted in Tempo, 1h left", "C-3 10m · not sent"]);
    expect(row.querySelector(".ub-row-hours")!.textContent).toBe("1h 2m");
    expect(within(row).getAllByRole("link").map((a) => [a.textContent, a.getAttribute("href")])).toEqual([["Fix Tempo line", "/2026-10-05"]]);
  });

  it("today's unsent work waits for the 17:00 send and has no fix button", () => {
    // catches: telling the Owner to fix work the 17:00 send will handle
    page(data([day({ day: TODAY, tempo: [gap({})] })]), 10);
    const card = document.getElementById(`day-${TODAY}`)!;
    expect(card.textContent).toContain("Sends to Tempo at 17:00 today");
    expect(within(card).queryByRole("link", { name: "Fix Tempo line" })).toBeNull();
  });

  it("ticketless work gets its own section with an Add ticket link per day", () => {
    // catches: hiding ticketless work, or a day with only ticketless work saying 'no open work'
    page(data([day({ day: "2026-10-10", ticketless_seconds: 7200 })]));
    const row = section(/No ticket yet/).querySelector(".ub-row") as HTMLElement;
    expect(row.dataset.state).toBe("ticket");
    expect(row.querySelector(".ub-row-hours")!.textContent).toBe("2h");
    expect(within(row).getAllByRole("link").map((a) => [a.textContent, a.getAttribute("href")])).toEqual([["Add ticket", "/2026-10-10"]]);
    expect(screen.getByLabelText("Sat 10 Oct: no ticket yet").getAttribute("href")).toBe("#ticket-2026-10-10");
  });

  it("export gaps sit last, collapsed, explained as optional, linking to Billing", () => {
    // catches: export back in the main flow, an open-by-default section, or a wrong Billing link
    page(data([day({ export: { seconds: 5400 } })]));
    const sec = section(/Not exported to the invoice form/);
    const details = sec.querySelector("details")!;
    expect(details.open).toBe(false);
    expect(sec.textContent).toContain("Only matters if you invoice through the Billing page export.");
    const row = details.querySelector(".ub-row") as HTMLElement;
    expect(row.id).toBe("bill-2026-10-05");
    expect(row.querySelector(".ub-row-hours")!.textContent).toBe("1h 30m");
    expect(within(row).getAllByRole("link").map((a) => a.getAttribute("href"))).toEqual(["/billing?from=2026-10-05"]);
    const order = [...document.querySelectorAll(".ub-sec h2")].map((h) => h.textContent);
    expect(order.at(-1)).toContain("Not exported to the invoice form");
  });

  it("a dismissed day shows no Tempo row but keeps its ticketless row", () => {
    // catches: listing a dismissed day under Tempo, or dropping its other open work
    page(data([day({ dismissed: "Klipping", ticketless_seconds: 600 })]));
    expect(screen.queryByRole("heading", { name: /Not in Tempo yet/ })).toBeNull();
    expect(section(/No ticket yet/).querySelector("#ticket-2026-10-05")).toBeTruthy();
  });

  it("the step summary shows open, Tempo-missing and ticketless totals; export is not a step", () => {
    // catches: export hours still counted as a step, or Tempo total using full lines for short ones
    page(
      data([
        day({ tempo: [gap({ seconds: 5400, in_tempo_seconds: 1800, reason: "short" })], export: { seconds: 5400 } }),
        day({ day: "2026-10-06", ticketless_seconds: 1800 }),
      ]),
    );
    const steps = screen.getByRole("list", { name: "How a day gets finished" });
    expect(steps.querySelectorAll("li")).toHaveLength(3);
    expect(steps.textContent).toContain("1h not in Tempo · 1 day");
    expect(steps.textContent).toContain("30m without a ticket · 1 day");
    expect(steps.textContent).toContain("1h 30m still open this cycle");
    expect(steps.textContent).not.toMatch(/export|billed/i);
  });

  it("shows when Tempo was last checked, and warns when that is over a day old", () => {
    // catches: hiding the data age, or trusting a stale pull without saying so
    page(data([], "2026-10-12T07:00:00Z"));
    const fresh = document.querySelector(".ub-pulled") as HTMLElement;
    expect(fresh.textContent).toContain("Tempo checked 2 hours ago");
    expect(fresh.dataset.stale).toBeUndefined();
    cleanup();
    page(data([], "2026-10-09T14:13:33Z"));
    const stale = document.querySelector(".ub-pulled") as HTMLElement;
    expect(stale.dataset.stale).toBe("true");
    expect(stale.textContent).toContain("Tempo checked 2 days ago");
    expect(within(stale).getByRole("link", { name: "Pull Tempo on the Logged page" }).getAttribute("href")).toBe("/logged");
    cleanup();
    page(data([], null));
    expect((document.querySelector(".ub-pulled") as HTMLElement).textContent).toContain("Tempo never checked");
  });

  it("the cycle strip names each day's state for screen readers and links open days", () => {
    // catches: a colour-only strip, or cells that do not jump to their row
    page(data([day({ export: { seconds: 60 } }), day({ day: "2026-10-06", tempo: [gap({})] })]));
    expect(screen.getByLabelText("Mon 5 Oct: not exported to the invoice form").getAttribute("href")).toBe("/billing?from=2026-10-05");
    expect(screen.getByLabelText("Tue 6 Oct: not in Tempo yet").getAttribute("href")).toBe("#day-2026-10-06");
    expect(screen.getByLabelText(/^Sun 20 Sept?: no open work$/).tagName).toBe("SPAN");
    expect(screen.getByLabelText("Tue 13 Oct: not yet").tagName).toBe("SPAN");
    const legend = screen.getByRole("list", { name: "Legend" });
    expect([...legend.querySelectorAll("li")].map((l) => l.dataset.state)).toEqual(["tempo", "ticket", "bill", "waiting", "clear"]);
  });
});
