// Shared fixtures for the TaskModal* / TaskActivity tests.

import { mock } from "bun:test";
import { render, screen } from "@testing-library/react";
import type { TaskRow, TicketDetail, TicketDraft, Transition } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { TaskModal } from "./TaskModal";

export const row = (over: Partial<TaskRow> = {}): TaskRow => ({
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  url: "https://x.atlassian.net/browse/ABC-1",
  assigned: true,
  week_seconds: 5400,
  today_seconds: 1800,
  last_worked_day: null,
  issue_type: null,
  priority: null,
  due_date: null,
  labels: [],
  parent_summary: null,
  updated: null,
  day_seconds: [0, 0, 0, 0, 0, 0, 0],
  ...over,
});

/** No hours this week and no blocks: the Comments tab is the default. */
export const quiet = (over: Partial<TaskRow> = {}) => row({ week_seconds: 0, today_seconds: 0, ...over });

export const detail = (over: Partial<TicketDetail> = {}): TicketDetail => ({
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  issue_type: "Bug",
  priority: "High",
  assignee: "Ada",
  updated: "2026-09-05T08:07:00",
  url: "https://jira.example/browse/ABC-1",
  description: "Steps to reproduce\nclick login",
  comments: [
    { id: "1", author: "Grace Hopper", created: "2026-09-04T09:00:00", body: "first one" },
    { id: "2", author: "Linus", created: "2026-09-05T09:30:00", body: "second one" },
  ],
  ...over,
});

export const start: Transition = { id: "11", name: "Start", to_status: "In Progress", to_category: "indeterminate" };
export const done: Transition = { id: "31", name: "Finish", to_status: "Done", to_category: "done" };

export const noDays = { key: "ABC-1", from: "2026-09-19", to: "2026-10-02", days: [] };

export function actions(over: Partial<Record<keyof TaskActions, unknown>> = {}): TaskActions {
  return {
    loadTransitions: mock(async () => ({ ok: true as const, data: [start, done] })),
    loadTicketDetail: mock(async () => ({ ok: true as const, data: detail() })),
    loadTicketBlocks: mock(async () => ({ ok: true as const, data: noDays })),
    transitionTicket: mock(async (key: string) => ({
      ok: true as const,
      data: { key, status: "In Progress", status_category: "indeterminate" as const },
    })),
    commentOnTicket: mock(async () => ({ ok: true as const, data: { ok: true as const } })),
    draftTicketUpdate: mock(async () => ({
      ok: true as const,
      data: {
        comment: "Fixed the login redirect.",
        suggested_transition_id: "31",
        transitions: [start, done],
      } satisfies TicketDraft,
    })),
    ...over,
  } as unknown as TaskActions;
}

export const calls = (fn: unknown) => (fn as ReturnType<typeof mock>).mock.calls;

export function open(a: TaskActions, task = row()) {
  const onClose = mock(() => {});
  const onStatus = mock((_s: unknown) => {});
  render(<TaskModal task={task} actions={a} onClose={onClose} onStatus={onStatus} />);
  return { onClose, onStatus };
}

export const dialog = () => screen.getByRole("dialog");
