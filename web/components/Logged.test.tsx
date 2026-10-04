// Logged views: fetch strip, month/week/day, header links, dismiss control.

import { afterEach, beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { shiftDay } from "@/lib/format";
import type { LoggedDay as Day, LoggedEntry, LoggedRange } from "@/lib/logged_contract";

const refresh = mock(() => {});
// Spread the real module: bun's mock.module is process-wide.
const realNavigation = { ...(await import("next/navigation")) };
mock.module("next/navigation", () => ({
  ...realNavigation,
  useRouter: () => ({ push: () => {}, refresh }),
}));
mock.module("./ThemeToggle", () => ({ ThemeToggle: () => null }));

type Res = { ok: true; data: LoggedRange } | { ok: false; error: string };
const refreshLogged = mock(async (_f: string, _t: string): Promise<Res> => ({ ok: false, error: "x" }));
const dismissLoggedDay = mock(async (_d: string, _r: string) => ({ ok: true as const, data: {} }));
const undismissLoggedDay = mock(async (_d: string) => ({ ok: true as const, data: {} }));
mock.module("@/app/actions-logged", () => ({ refreshLogged, dismissLoggedDay, undismissLoggedDay }));

let LoggedFetch: typeof import("./LoggedFetch").LoggedFetch;
let LoggedMonth: typeof import("./LoggedMonth").LoggedMonth;
let LoggedWeek: typeof import("./LoggedWeek").LoggedWeek;
let LoggedDay: typeof import("./LoggedDay").LoggedDay;
let LoggedHeader: typeof import("./LoggedHeader").LoggedHeader;
let DismissDay: typeof import("./DismissDay").DismissDay;

beforeAll(async () => {
  LoggedFetch = (await import("./LoggedFetch")).LoggedFetch;
  LoggedMonth = (await import("./LoggedMonth")).LoggedMonth;
  LoggedWeek = (await import("./LoggedWeek")).LoggedWeek;
  LoggedDay = (await import("./LoggedDay")).LoggedDay;
  LoggedHeader = (await import("./LoggedHeader")).LoggedHeader;
  DismissDay = (await import("./DismissDay")).DismissDay;
});

beforeEach(() => {
  for (const m of [refresh, refreshLogged, dismissLoggedDay, undismissLoggedDay]) m.mockClear();
});
afterEach(cleanup);

const TODAY = "2026-10-04";
const PULLED = "2026-10-04T14:02:00Z";
const entry = (over: Partial<LoggedEntry> = {}): LoggedEntry => ({
  tempo_worklog_id: "w1",
  issue_id: 77,
  jira_issue: "PROJ-12",
  seconds: 7200,
  description: "Fixed the export race",
  owner: "worklog",
  ...over,
});
const day = (d: string, over: Partial<Day> = {}): Day => ({
  day: d,
  logged_seconds: 28800,
  required_seconds: 28800,
  state: "full",
  dismissal_reason: null,
  entries: [],
  ...over,
});
const range = (days: Day[], over: Partial<LoggedRange> = {}): LoggedRange => ({
  from: days[0].day,
  to: days[days.length - 1].day,
  today: TODAY,
  days,
  pulled_at: PULLED,
  ...over,
});
const text = () => document.body.textContent ?? "";

describe("LoggedFetch", () => {
  it("shows stored data before the fetch resolves and fetches once on mount", async () => {
    let resolve!: (r: Res) => void;
    refreshLogged.mockImplementationOnce(() => new Promise<Res>((r) => (resolve = r)));
    render(<LoggedFetch from="2026-09-28" to="2026-10-04" pulledAt={PULLED} />);
    expect(text()).toContain("Fetching from Tempo…");
    expect(text()).toContain("Showing data from");
    expect(refreshLogged.mock.calls).toEqual([["2026-09-28", "2026-10-04"]]);
    expect(screen.getByRole("button", { name: /Refresh from Tempo/ }).hasAttribute("disabled")).toBe(true);

    await act(async () => resolve({ ok: true, data: range([day("2026-09-28")]) }));
    expect(text()).toContain("From Tempo · updated");
    expect(refresh.mock.calls.length).toBe(1);
    expect(refreshLogged.mock.calls.length).toBe(1);
  });

  it("calls refreshLogged again when Refresh is clicked", async () => {
    refreshLogged.mockImplementation(async () => ({ ok: true, data: range([day("2026-10-01")]) }));
    render(<LoggedFetch from="a" to="b" pulledAt={PULLED} />);
    const btn = screen.getByRole("button", { name: /Refresh from Tempo/ });
    await waitFor(() => expect(btn.hasAttribute("disabled")).toBe(false));
    fireEvent.click(btn);
    await waitFor(() => expect(refreshLogged.mock.calls.length).toBe(2));
  });

  it("failed refresh with stored data says so and keeps the raw error in the title only", async () => {
    refreshLogged.mockImplementationOnce(async () => ({ ok: false, error: "boom 500" }));
    render(<LoggedFetch from="a" to="b" pulledAt={PULLED} />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Couldn't reach Tempo — showing data from");
    expect(alert.textContent).toContain("Try Refresh again in a minute.");
    expect(alert.textContent).not.toContain("boom");
    expect(alert.getAttribute("title")).toBe("boom 500");
  });

  it("failed refresh with nothing stored says nothing stored yet", async () => {
    refreshLogged.mockImplementationOnce(async () => ({ ok: false, error: "request timed out" }));
    render(<LoggedFetch from="a" to="b" pulledAt={null} />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("nothing stored yet");
    expect(alert.textContent).toContain("Tempo took too long — try Refresh again.");
  });

  it("maps a token error to the Settings sentence", async () => {
    refreshLogged.mockImplementationOnce(async () => ({ ok: false, error: "tempo 401 Unauthorized" }));
    render(<LoggedFetch from="a" to="b" pulledAt={null} />);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Tempo didn't accept the token — check it in Settings.");
  });
});

// Mon 28 Sep .. Sun 1 Nov: 5 whole weeks around October 2026.
const GRID = Array.from({ length: 35 }, (_, i) => shiftDay("2026-09-28", i));

describe("LoggedMonth", () => {
  const states: Record<string, Partial<Day>> = {
    "2026-09-30": { state: "under", logged_seconds: 18000 },
    "2026-10-10": { state: "off", logged_seconds: 0, required_seconds: null },
    "2026-10-15": { state: "not_fetched", logged_seconds: 0 },
    "2026-10-04": { state: "pending", logged_seconds: 0 },
  };
  const renderMonth = () =>
    render(<LoggedMonth month="2026-10" range={range(GRID.map((d) => day(d, states[d])))} />);

  it("renders all 35 days as links to the day view with hours and required", () => {
    renderMonth();
    const cells = [...document.querySelectorAll("a.logged-cell")];
    expect(cells.length).toBe(35);
    expect(cells.map((c) => c.getAttribute("href"))).toEqual(GRID.map((d) => `/logged/day/${d}`));
    expect(cells[1].textContent).toContain("8h");
    expect(cells[1].textContent).toContain("of 8h");
  });

  it("flags an under day as Short with a rail state and lists it", () => {
    renderMonth();
    const cell = document.querySelector('a[href="/logged/day/2026-09-30"]')!;
    expect(cell.getAttribute("data-state")).toBe("under");
    expect(cell.textContent).toContain("Short");
    expect(cell.textContent).toContain("5h");
    expect(cell.getAttribute("aria-label")).toBe("Wednesday 30 September: 5h of 8h, short");
    // Outside the month (30 Sep) so not in the Short list.
    expect(document.querySelector(".logged-short")).toBeNull();
  });

  it("lists in-month short days under the header strip", () => {
    render(
      <LoggedMonth
        month="2026-10"
        range={range(GRID.map((d) => day(d, d === "2026-10-02" ? { state: "under", logged_seconds: 27000 } : {})))}
      />,
    );
    const list = document.querySelector(".logged-short")!;
    expect(list.textContent).toBe("Short: Fri 2 Oct");
    expect(list.querySelector("a")!.getAttribute("href")).toBe("/logged/day/2026-10-02");
  });

  it("shows an em dash and 'not fetched' for a not_fetched day, never 0h", () => {
    renderMonth();
    const cell = document.querySelector('a[href="/logged/day/2026-10-15"]')!;
    expect(cell.textContent).toContain("—");
    expect(cell.textContent).toContain("not fetched");
    expect(cell.textContent).not.toContain("0h");
    expect(cell.getAttribute("aria-label")).toBe("Thursday 15 October: not fetched yet");
  });

  it("marks today and hides hours on an empty day off", () => {
    renderMonth();
    const today = document.querySelector('a[href="/logged/day/2026-10-04"]')!;
    expect(today.textContent).toContain("today");
    expect(today.querySelector('[aria-current="date"]')).toBeTruthy();
    const off = document.querySelector('a[href="/logged/day/2026-10-10"]')!;
    expect(off.querySelector(".logged-cell-hours")).toBeNull();
    expect(off.textContent).toContain("off");
  });
});

describe("LoggedWeek and LoggedDay", () => {
  const WEEK = Array.from({ length: 7 }, (_, i) => shiftDay("2026-09-28", i));
  const e2 = entry({ tempo_worklog_id: "w2", jira_issue: null, issue_id: 4242, seconds: 10800, description: "", owner: "outside" });

  it("renders the 7 days of a week with totals and entries", () => {
    const days = WEEK.map((d) => day(d, d === "2026-09-30" ? { state: "under", logged_seconds: 18000, entries: [entry(), e2] } : {}));
    render(<LoggedWeek range={range(days)} />);
    expect(document.querySelectorAll(".logged-week-day").length).toBe(7);
    const wed = document.querySelector('.logged-week-day[data-state="under"]')!;
    expect(wed.textContent).toContain("Wed 30 Sep");
    expect(wed.textContent).toContain("5h of 8h");
    expect(wed.textContent).toContain("PROJ-12");
    expect(wed.textContent).toContain("Is this day filled out?");
  });

  it("shows ticket, hours, description and who sent the entry on the day view", () => {
    render(<LoggedDay range={range([day("2026-09-30", { state: "under", logged_seconds: 18000, entries: [entry(), e2] })])} />);
    const [a, b] = [...document.querySelectorAll(".logged-entries li")];
    expect(a.textContent).toContain("PROJ-12");
    expect(a.textContent).toContain("2h");
    expect(a.textContent).toContain("Fixed the export race");
    expect(a.textContent).toContain("sent by worklog");
    expect(b.textContent).toContain("Tempo #4242");
    expect(b.textContent).toContain("3h");
    expect(b.textContent).toContain("No description");
    expect(b.textContent).toContain("hand-logged");
  });

  it("says nothing was logged on an empty past day and not fetched otherwise", () => {
    const { unmount } = render(<LoggedDay range={range([day("2026-10-01", { state: "off", logged_seconds: 0 })])} />);
    expect(text()).toContain("Nothing logged in Tempo for this day.");
    expect(text()).toContain("Log time in Tempo, then press Refresh from Tempo.");
    unmount();
    render(<LoggedDay range={range([day("2026-10-01", { state: "not_fetched", logged_seconds: 0 })])} />);
    expect(text()).toContain("Not fetched from Tempo yet.");
    expect(text()).not.toContain("0h");
  });
});

describe("LoggedHeader", () => {
  const hrefs = () => [...document.querySelectorAll("a")].map((a) => a.getAttribute("href"));

  it("month: prev/next by month, this-month disabled in the current month, view switch anchors on today", () => {
    render(<LoggedHeader view="month" id="2026-10" range={range(GRID.map((d) => day(d)))} />);
    expect(hrefs()).toEqual([
      "/logged/month/2026-10",
      "/logged/week/2026-09-28",
      "/logged/day/2026-10-04",
      "/logged/month/2026-09",
      "/logged/month/2026-11",
    ]);
    expect(screen.getByText("This month").getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByRole("heading").textContent).toBe("October 2026");
  });

  it("week: prev/next by week and a This week link when elsewhere", () => {
    render(<LoggedHeader view="week" id="2026-09-21" range={range(Array.from({ length: 7 }, (_, i) => day(shiftDay("2026-09-21", i))))} />);
    expect(hrefs()).toContain("/logged/week/2026-09-14");
    expect(hrefs()).toContain("/logged/week/2026-09-28");
    expect(screen.getByText("This week").getAttribute("href")).toBe("/logged/week/2026-09-28");
    // Anchor is the shown Monday when today is outside the week.
    expect(hrefs()).toContain("/logged/month/2026-09");
    expect(hrefs()).toContain("/logged/day/2026-09-21");
  });

  it("day: prev/next by day, no summary line, and counts short days for a week", () => {
    render(<LoggedHeader view="day" id="2026-10-01" range={range([day("2026-10-01")])} />);
    expect(hrefs()).toContain("/logged/day/2026-09-30");
    expect(hrefs()).toContain("/logged/day/2026-10-02");
    expect(screen.getByRole("heading").textContent).toBe("Thursday, 1 October 2026");
    expect(document.querySelector(".day-total")).toBeNull();
    cleanup();
    const days = Array.from({ length: 7 }, (_, i) => day(shiftDay("2026-09-28", i), i < 2 ? { state: "under", logged_seconds: 14400 } : {}));
    render(<LoggedHeader view="week" id="2026-09-28" range={range(days)} />);
    expect(document.querySelector(".day-total")!.textContent).toBe("48h logged · 56h required · 2 short");
  });
});

describe("DismissDay", () => {
  const show = (over: Partial<Parameters<typeof DismissDay>[0]> = {}) =>
    render(<DismissDay day="2026-09-30" state="under" reason={null} loggedSeconds={18000} requiredSeconds={28800} {...over} />);
  const open = () => {
    fireEvent.click(screen.getByRole("button", { name: "Mark as fine…" }));
    return screen.getByLabelText("Why is this day short?") as HTMLInputElement;
  };

  it("asks, then opens a form with focus in the input", () => {
    show();
    expect(text()).toContain("Is this day filled out?");
    expect(text()).toContain("5h of 8h logged.");
    const input = open();
    expect(document.activeElement).toBe(input);
  });

  it("shows a hint and sends nothing for an empty reason", () => {
    show();
    const input = open();
    fireEvent.click(screen.getByRole("button", { name: "Save reason" }));
    expect(text()).toContain("Write a short reason, like “dentist”.");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(dismissLoggedDay.mock.calls.length).toBe(0);
  });

  it("shows a hint with the count for 81 characters and sends nothing", () => {
    show();
    const input = open();
    fireEvent.change(input, { target: { value: "x".repeat(81) } });
    fireEvent.click(screen.getByRole("button", { name: "Save reason" }));
    expect(text()).toContain("Keep it to 80 characters (now 81).");
    expect(dismissLoggedDay.mock.calls.length).toBe(0);
  });

  it("sends the trimmed reason and refreshes", async () => {
    show();
    fireEvent.change(open(), { target: { value: " dentist " } });
    fireEvent.click(screen.getByRole("button", { name: "Save reason" }));
    await waitFor(() => expect(dismissLoggedDay.mock.calls).toEqual([["2026-09-30", "dentist"]]));
    await waitFor(() => expect(refresh.mock.calls.length).toBe(1));
  });

  it("keeps the input and shows the daemon error when saving fails", async () => {
    dismissLoggedDay.mockImplementationOnce(async () => ({ ok: false, error: "db locked" }) as never);
    show();
    const input = open();
    fireEvent.change(input, { target: { value: "dentist" } });
    fireEvent.click(screen.getByRole("button", { name: "Save reason" }));
    expect((await screen.findByRole("alert")).textContent).toBe("db locked");
    expect(input.value).toBe("dentist");
  });

  it("Cancel closes and returns focus to Mark as fine", () => {
    show();
    open();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Mark as fine…" }));
  });

  it("dismissed state shows the reason and Undo calls undismissLoggedDay", async () => {
    show({ state: "dismissed", reason: "dentist" });
    expect(text()).toContain("Marked fine: dentist");
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(undismissLoggedDay.mock.calls).toEqual([["2026-09-30"]]));
    await waitFor(() => expect(refresh.mock.calls.length).toBe(1));
  });
});
