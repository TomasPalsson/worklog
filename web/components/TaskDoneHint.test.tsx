import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { StatusHint, TaskRow } from "@/lib/types";
import { TaskCard } from "./TaskCard";
import { TaskDoneHint } from "./TaskDoneHint";

afterEach(cleanup);

const hint: StatusHint = {
  key: "GENAI-7",
  summary: "Ship it",
  to_category: "done",
  reason: { kind: "pr_merged", repo: "acme/api", number: 42, merged_at: "2026-10-01T10:00:00Z" },
};

describe("TaskDoneHint", () => {
  it("asks first, then moves only on confirm", () => {
    const onConfirm = mock(() => {});
    render(<TaskDoneHint hint={hint} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("button", { name: "Move to Done?" }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByText(/acme\/api#42 merged/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Move to Done" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("not now returns to the chip without moving", () => {
    const onConfirm = mock(() => {});
    render(<TaskDoneHint hint={hint} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("button", { name: "Move to Done?" }));
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.getByRole("button", { name: "Move to Done?" })).toBeTruthy();
    expect(onConfirm).not.toHaveBeenCalled();
  });
});

const task = (over: Partial<TaskRow>): TaskRow => ({
  key: "GENAI-7", summary: "Ship it", status: "In Progress", status_category: "indeterminate", url: null,
  assigned: true, week_seconds: 0, today_seconds: 0, last_worked_day: null, issue_type: null, priority: null,
  due_date: null, labels: [], parent_summary: null, updated: null, day_seconds: [], ...over,
});

const card = (t: TaskRow, column: "new" | "indeterminate" | "done", onMove = mock(() => {})) =>
  render(
    <TaskCard
      task={t} column={column} selected={false} pending={false} dragging={false} landed={false}
      error={undefined} onRetry={() => {}} onOpen={() => {}} loadTransitions={async () => ({ ok: true, data: [] })}
      onMove={onMove} onDismissError={() => {}} onDragStart={() => {}} onDragEnd={() => {}}
    />,
  );

describe("TaskCard done hint", () => {
  it("confirming moves the card to the done column", () => {
    const onMove = mock(() => {});
    card(task({ done_hint: hint }), "indeterminate", onMove);
    fireEvent.click(screen.getByRole("button", { name: "Move to Done?" }));
    fireEvent.click(screen.getByRole("button", { name: "Move to Done" }));
    expect(onMove).toHaveBeenCalledWith("done");
  });

  it("no chip without a hint or in the done column", () => {
    card(task({}), "indeterminate");
    expect(screen.queryByRole("button", { name: "Move to Done?" })).toBeNull();
    cleanup();
    card(task({ done_hint: hint }), "done");
    expect(screen.queryByRole("button", { name: "Move to Done?" })).toBeNull();
  });
});
