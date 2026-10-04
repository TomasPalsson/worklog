// B12, B15, B17: week close-out panel — gap flag and the confirmed Sync week loop.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { CloseoutDay, WeekCloseout as Closeout } from "@/lib/types";
import { WeekCloseout, type CloseoutActions } from "./WeekCloseout";

afterEach(cleanup);

const H = 3600;
const day = (d: string, over: Partial<CloseoutDay>): CloseoutDay => ({
  day: d,
  logged_seconds: 8 * H,
  synced_seconds: 8 * H,
  tempo_seconds: 8 * H,
  outside_seconds: 0,
  required_seconds: 8 * H,
  unticketed_seconds: 0,
  pending_lines: 0,
  ...over,
});
const week = (days: CloseoutDay[], pulled_at: string | null = "2026-09-24T10:00:00Z"): Closeout => ({
  monday: "2026-09-21",
  days,
  pulled_at,
});
const report = { monday: "2026-09-21", worklogs: 0, outside: 0, schedule_days: 5, pulled_at: "t" };

function actions(over: Partial<CloseoutActions> = {}, next: Closeout = week([])): CloseoutActions {
  return {
    pull: mock(async () => ({ ok: true as const, data: report })),
    syncDay: mock(async () => ({ ok: true as const, data: { synced: 1, skipped: 0, errors: [] } })),
    loadCloseout: mock(async () => ({ ok: true as const, data: next })),
    ...over,
  };
}

const row = (d: string) => screen.getByTestId(`closeout-${d}`);

describe("WeekCloseout gap flag (B17)", () => {
  it("flags 7.5h in Tempo against 8h required", () => {
    render(<WeekCloseout closeout={week([day("2026-09-21", { tempo_seconds: 7.5 * H })])} actions={actions()} />);
    expect(row("2026-09-21").textContent).toContain("Gap");
  });

  it("gives a gap row the warning class and a clean row none", () => {
    render(
      <WeekCloseout
        closeout={week([day("2026-09-21", { tempo_seconds: 7.5 * H }), day("2026-09-22", {})])}
        actions={actions()}
      />,
    );
    expect(row("2026-09-21").className).toContain("closeout-gap");
    expect(row("2026-09-22").className).not.toContain("closeout-gap");
  });

  it("does not flag exactly 8h", () => {
    render(<WeekCloseout closeout={week([day("2026-09-21", {})])} actions={actions()} />);
    expect(row("2026-09-21").textContent).not.toContain("Gap");
    expect(row("2026-09-21").textContent).toContain("✓");
  });

  it("does not flag a day that requires 0h", () => {
    render(
      <WeekCloseout
        closeout={week([day("2026-09-26", { required_seconds: 0, tempo_seconds: 0, logged_seconds: 0 })])}
        actions={actions()}
      />,
    );
    expect(row("2026-09-26").textContent).not.toContain("Gap");
  });

  it("shows 'not pulled' when required is unknown and never flags it", () => {
    render(
      <WeekCloseout closeout={week([day("2026-09-21", { required_seconds: null, tempo_seconds: 0 })], null)} actions={actions()} />,
    );
    expect(row("2026-09-21").textContent).toContain("not pulled");
    expect(row("2026-09-21").textContent).not.toContain("Gap");
  });

  it("a day with pending lines gets no check mark", () => {
    render(<WeekCloseout closeout={week([day("2026-09-21", { pending_lines: 2 })])} actions={actions()} />);
    expect(row("2026-09-21").textContent).not.toContain("✓");
  });
});

describe("WeekCloseout day card", () => {
  it("a future day shows no Gap chip and no done mark", () => {
    render(
      <WeekCloseout closeout={week([day("2999-01-01", { tempo_seconds: 0, logged_seconds: 0 })])} actions={actions()} />,
    );
    expect(row("2999-01-01").textContent).not.toContain("Gap");
    expect(row("2999-01-01").textContent).not.toContain("✓ done");
  });

  it("no done mark while unticketed time remains", () => {
    render(<WeekCloseout closeout={week([day("2026-09-21", { unticketed_seconds: 1.9 * H })])} actions={actions()} />);
    expect(row("2026-09-21").textContent).toContain("no ticket");
    expect(row("2026-09-21").textContent).not.toContain("✓ done");
  });

  it("the big number uses the passed day work seconds", () => {
    render(
      <WeekCloseout
        closeout={week([day("2026-09-21", { logged_seconds: 2 * H })])}
        daySeconds={{ "2026-09-21": 6 * H }}
        actions={actions()}
      />,
    );
    expect(row("2026-09-21").querySelector(".week-day-hours")?.textContent).toBe("6h");
  });
});

describe("WeekCloseout Sync week (B12, B15)", () => {
  const days = [
    day("2026-09-21", { pending_lines: 1 }),
    day("2026-09-22", {}),
    day("2026-09-23", { pending_lines: 2 }),
  ];

  it("needs a second click, then syncs only days with pending lines in order", async () => {
    const a = actions();
    render(<WeekCloseout closeout={week(days)} actions={a} />);
    fireEvent.click(screen.getByRole("button", { name: "Sync week" }));
    expect(a.syncDay).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm sync week" }));
    await waitFor(() => expect(a.loadCloseout).toHaveBeenCalled());
    expect((a.syncDay as ReturnType<typeof mock>).mock.calls.map((c) => c[0])).toEqual(["2026-09-21", "2026-09-23"]);
  });

  it("names the failed day and its error and does not send later days", async () => {
    const a = actions({
      syncDay: mock(async (d: string) =>
        d === "2026-09-21" ? { ok: false as const, error: "Tempo 502" } : { ok: true as const, data: { synced: 1, skipped: 0, errors: [] } },
      ),
    });
    render(<WeekCloseout closeout={week(days)} actions={a} />);
    fireEvent.click(screen.getByRole("button", { name: "Sync week" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm sync week" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("2026-09-21");
    expect(alert.textContent).toContain("Tempo 502");
    expect(a.syncDay).toHaveBeenCalledTimes(1);
  });

  it("shows the pull error and keeps the numbers when Pull from Tempo fails", async () => {
    const a = actions({ pull: mock(async () => ({ ok: false as const, error: "no token" })) });
    render(<WeekCloseout closeout={week(days)} actions={a} />);
    fireEvent.click(screen.getByRole("button", { name: "Pull from Tempo" }));
    expect((await screen.findByRole("alert")).textContent).toContain("no token");
    expect(row("2026-09-23").textContent).toContain("2");
  });

  it("disables both buttons while a pull is in flight", async () => {
    let release: (v: { ok: true; data: typeof report }) => void = () => {};
    const a = actions({ pull: mock(() => new Promise((r) => { release = r; })) as CloseoutActions["pull"] });
    render(<WeekCloseout closeout={week(days)} actions={a} />);
    fireEvent.click(screen.getByRole("button", { name: "Pull from Tempo" }));
    await waitFor(() => expect((screen.getByRole("button", { name: "Sync week" }) as HTMLButtonElement).disabled).toBe(true));
    expect((screen.getByRole("button", { name: "Pulling…" }) as HTMLButtonElement).disabled).toBe(true);
    release({ ok: true, data: report });
    await waitFor(() => expect(a.loadCloseout).toHaveBeenCalled());
  });
});
