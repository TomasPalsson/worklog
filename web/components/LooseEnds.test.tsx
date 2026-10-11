import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { LooseEnd, LooseEnds as LooseEndsData } from "@/lib/time_leaks_contract";
import { LooseEnds } from "./LooseEnds";

afterEach(cleanup);

const item = (over: Partial<LooseEnd>): LooseEnd => ({
  block_id: 1,
  day: "2026-10-05",
  jira_issue: "ABC-1",
  started_at: "2026-10-05T09:00:00Z",
  ended_at: "2026-10-05T10:30:00Z",
  seconds: 5400,
  description: null,
  missing: ["tempo"],
  ...over,
});
const data = (items: LooseEnd[]): LooseEndsData => ({ cycle_start: "2026-09-20", cycle_end: "2026-10-19", items });

describe("LooseEnds", () => {
  it("says the list could not load when the daemon call failed", () => {
    // catches: swallowing the error into an empty list
    render(<LooseEnds data={null} />);
    expect(screen.getByRole("alert").textContent).toContain("could not be loaded");
    expect(screen.queryByText(/nothing/i)).toBeNull();
  });

  it("says nothing is loose when the list is empty", () => {
    // catches: rendering a blank page or an error for an empty cycle
    render(<LooseEnds data={data([])} />);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText(/nothing/i)).toBeTruthy();
  });

  it("renders one row per item", () => {
    // catches: rendering only the first item
    render(<LooseEnds data={data([item({ block_id: 1 }), item({ block_id: 2, jira_issue: "ABC-2" }), item({ block_id: 3, jira_issue: "ABC-3" })])} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
  });

  it("links each row to its own day page", () => {
    // catches: linking every row to today or the cycle start
    render(<LooseEnds data={data([item({ day: "2026-10-05" }), item({ block_id: 2, day: "2026-10-07", jira_issue: "ABC-2" })])} />);
    const hrefs = screen.getAllByRole("link").map((a) => a.getAttribute("href"));
    expect(hrefs).toEqual(["/2026-10-05", "/2026-10-07"]);
  });

  it("names only the missing Tempo id", () => {
    // catches: always listing both gaps
    render(<LooseEnds data={data([item({ missing: ["tempo"] })])} />);
    const row = screen.getByRole("listitem").textContent ?? "";
    expect(row).toContain("Tempo id");
    expect(row).not.toContain("export date");
  });

  it("names only the missing export date", () => {
    // catches: tempo/export labels swapped or collapsed
    render(<LooseEnds data={data([item({ missing: ["export"] })])} />);
    const row = screen.getByRole("listitem").textContent ?? "";
    expect(row).toContain("export date");
    expect(row).not.toContain("Tempo id");
  });

  it("names both when both are missing", () => {
    // catches: showing only the first missing item
    render(<LooseEnds data={data([item({ missing: ["tempo", "export"] })])} />);
    const row = screen.getByRole("listitem").textContent ?? "";
    expect(row).toContain("Tempo id");
    expect(row).toContain("export date");
  });

  it("shows the ticket, duration and cycle bounds", () => {
    // catches: dropping the ticket or showing raw seconds
    render(<LooseEnds data={data([item({ jira_issue: "XYZ-9", seconds: 5400 })])} />);
    expect(screen.getByRole("listitem").textContent).toContain("XYZ-9");
    expect(screen.getByRole("listitem").textContent).toContain("1h 30m");
    expect(document.body.textContent).toContain("2026-09-20");
    expect(document.body.textContent).toContain("2026-10-19");
  });
});
