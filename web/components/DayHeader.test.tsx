// "Today" / "This week" are always rendered so the arrows never shift under
// a fast double-click; the date picker jumps to a day (or its week).

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { mondayOf, todayISO } from "@/lib/format";

const push = mock((_href: string) => {});
mock.module("next/navigation", () => ({
  useRouter: () => ({ push, refresh: mock(() => {}) }),
  usePathname: () => "/",
}));

let DayHeader: typeof import("./DayHeader").DayHeader;
let DateJumper: typeof import("./DateJumper").DateJumper;
let WeekHeader: typeof import("./WeekHeader").WeekHeader;

beforeAll(async () => {
  DayHeader = (await import("./DayHeader")).DayHeader;
  DateJumper = (await import("./DateJumper")).DateJumper;
  WeekHeader = (await import("./WeekHeader")).WeekHeader;
});

afterEach(() => {
  cleanup();
  push.mockClear();
});

const header = (day: string) =>
  render(
    <DayHeader
      day={day}
      heading={day}
      totalHours="0h"
      blockCount={0}
      unassigned={0}
    />,
  );

const navLabels = () =>
  Array.from(screen.getByRole("navigation", { name: "day navigation" }).children).map(
    (el) => el.getAttribute("aria-label") ?? el.textContent,
  );

describe("DayHeader nav", () => {
  it("renders Today on today too, disabled, so the arrows keep their slots", () => {
    header(todayISO());
    const today = screen.getByText("Today");
    expect(today.tagName).toBe("SPAN");
    expect(today.getAttribute("aria-disabled")).toBe("true");
    expect(navLabels().slice(0, 3)).toEqual(["previous day", "Today", "next day"]);
  });

  it("links Today on another day, in the same slot", () => {
    header("2026-01-05");
    const today = screen.getByText("Today");
    expect(today.tagName).toBe("A");
    expect(today.getAttribute("href")).toBe(`/${todayISO()}`);
    expect(navLabels().slice(0, 3)).toEqual(["previous day", "Today", "next day"]);
  });

  it("date picker opens on the shown day and jumps to the picked day", () => {
    header("2026-01-05");
    const input = screen.getByLabelText("jump to date") as HTMLInputElement;
    expect(input.value).toBe("2026-01-05");
    fireEvent.change(input, { target: { value: "2025-12-24" } });
    expect(push).toHaveBeenCalledWith("/2025-12-24");
  });
});

describe("DateJumper", () => {
  it("week view jumps to the picked day's Monday", () => {
    render(<DateJumper focusedDay="2026-01-05" view="week" />);
    fireEvent.change(screen.getByLabelText("jump to date"), {
      target: { value: "2025-12-24" },
    });
    expect(push).toHaveBeenCalledWith("/week/2025-12-22");
  });

  it("re-seeds when the shown day changes without a remount", () => {
    const { rerender } = render(<DateJumper focusedDay="2026-01-05" view="day" />);
    fireEvent.change(screen.getByLabelText("jump to date"), {
      target: { value: "2025-12-24" },
    });
    rerender(<DateJumper focusedDay="2025-12-23" view="day" />);
    expect((screen.getByLabelText("jump to date") as HTMLInputElement).value).toBe(
      "2025-12-23",
    );
  });

  it("ignores the partial years keyboard entry passes through", () => {
    render(<DateJumper focusedDay="2026-01-05" view="day" />);
    const input = screen.getByLabelText("jump to date");
    for (const y of ["0002", "0020", "0202"]) {
      fireEvent.change(input, { target: { value: `${y}-01-05` } });
    }
    expect(push).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "2025-01-05" } });
    expect(push).toHaveBeenCalledWith("/2025-01-05");
  });

  it("ignores a cleared date and the day already shown", () => {
    render(<DateJumper focusedDay="2026-01-05" view="day" />);
    const input = screen.getByLabelText("jump to date");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.change(input, { target: { value: "2026-01-05" } });
    expect(push).not.toHaveBeenCalled();
  });
});

describe("WeekHeader nav", () => {
  const labels = () =>
    Array.from(screen.getByRole("navigation", { name: "week navigation" }).children).map(
      (el) => el.getAttribute("aria-label") ?? el.textContent,
    );

  it("keeps This week in its slot, disabled on the current week", () => {
    render(<WeekHeader monday={mondayOf(todayISO())} workSeconds={0} workBlocks={0} />);
    expect(screen.getByText("This week").tagName).toBe("SPAN");
    expect(labels().slice(0, 3)).toEqual(["previous week", "This week", "next week"]);
    cleanup();
    render(<WeekHeader monday="2026-01-05" workSeconds={0} workBlocks={0} />);
    expect(screen.getByText("This week").tagName).toBe("A");
    expect(labels().slice(0, 3)).toEqual(["previous week", "This week", "next week"]);
  });
});

describe("headers carry only date controls (menu lives in AppNav)", () => {
  it("day header has no view toggle, export, Billing or Tasks link", () => {
    const { container } = header("2026-01-05");
    expect(container.querySelector("a[href^='/billing']")).toBeNull();
    expect(container.querySelector("a[href='/tasks']")).toBeNull();
    expect(screen.queryByRole("link", { name: /week/i })).toBeNull();
    expect(container.querySelector("[aria-label='Day view']")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(navLabels().slice(0, 3)).toEqual(["previous day", "Today", "next day"]);
  });

  it("week header has no Tasks or Day link", () => {
    const { container } = render(
      <WeekHeader monday="2026-01-05" workSeconds={0} workBlocks={0} />,
    );
    expect(container.querySelector("a[href='/tasks']")).toBeNull();
    expect(screen.queryByRole("link", { name: "switch to day view" })).toBeNull();
  });
});
