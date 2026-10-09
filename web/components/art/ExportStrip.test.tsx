import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ExportStrip, segLabel, segState } from "./ExportStrip";

afterEach(cleanup);

const row = (over: Partial<Parameters<typeof segState>[0]> = {}) => ({
  hours: 2.5,
  customer: "Acme",
  verkefni: "Dev",
  needs_description: false,
  folder: "acme",
  ...over,
});

describe("segState", () => {
  it("done wins over missing data", () => {
    expect(segState(row({ customer: null }), true)).toBe("done");
  });
  it("missing customer, deild or description needs you", () => {
    expect(segState(row({ customer: null }), false)).toBe("needs");
    expect(segState(row({ verkefni: null }), false)).toBe("needs");
    expect(segState(row({ needs_description: true }), false)).toBe("needs");
  });
  it("complete and not done is to do", () => {
    expect(segState(row(), false)).toBe("todo");
  });
});

describe("ExportStrip", () => {
  it("weights segments by hours, tiny lines keep a 6px floor via CSS min-width", () => {
    const rows = [row({ hours: 4 }), row({ hours: 0.5 }), row({ hours: 0 })];
    const { container } = render(<ExportStrip rows={rows} index={0} done={new Set()} onSelect={() => {}} />);
    const grows = [...container.querySelectorAll("li")].map((li) => li.style.flexGrow);
    expect(grows).toEqual(["4", "0.5", "0"]);
  });

  it("labels each segment with line, name, hours and state; falls back to folder", () => {
    const rows = [row(), row({ customer: null, hours: 1 })];
    render(<ExportStrip rows={rows} index={0} done={new Set([0])} onSelect={() => {}} />);
    expect(screen.getByLabelText("Line 1 of 2: Acme, 2.5h, done")).toBeTruthy();
    expect(screen.getByLabelText("Line 2 of 2: acme, 1h, needs you")).toBeTruthy();
    expect(segLabel(row(), 2, 9, "todo")).toBe("Line 3 of 9: Acme, 2.5h, to do");
  });

  it("marks the current line and jumps on click", () => {
    const onSelect = mock((_: number) => {});
    render(<ExportStrip rows={[row(), row()]} index={1} done={new Set()} onSelect={onSelect} />);
    const first = screen.getByLabelText(/^Line 1 of 2/);
    expect(screen.getByLabelText(/^Line 2 of 2/).getAttribute("aria-current")).toBe("true");
    expect(first.getAttribute("aria-current")).toBeNull();
    fireEvent.click(first);
    expect(onSelect).toHaveBeenCalledWith(0);
  });

  it("legend counts lines needing a pick, none when all fine", () => {
    const { rerender } = render(<ExportStrip rows={[row({ customer: null }), row()]} index={0} done={new Set()} onSelect={() => {}} />);
    expect(screen.getByText("1 need a pick")).toBeTruthy();
    rerender(<ExportStrip rows={[row()]} index={0} done={new Set()} onSelect={() => {}} />);
    expect(screen.queryByText(/need a pick/)).toBeNull();
    rerender(<ExportStrip rows={[row({ needs_description: true })]} index={0} done={new Set()} onSelect={() => {}} />);
    expect(screen.queryByText(/need a pick/)).toBeNull();
  });
});
