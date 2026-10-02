// Tests for the Tempo hub Server Actions (actions-hub.ts).
// Mirrors actions-tempo-lines.test.ts: mock next/cache and the daemon client.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import type {
  PullReport,
  RawBlock,
  TicketBlocks,
  TasksResponse,
  TicketDetail,
  TicketDraft,
  TicketStatus,
  Transition,
  WeekCloseout,
} from "@/lib/types";

const revalidateImpl = mock((_path: string) => {});
mock.module("next/cache", () => ({
  revalidatePath: (path: string) => revalidateImpl(path),
}));

const tasksBody: TasksResponse = {
  monday: "2026-09-21",
  today: "2026-09-24",
  tasks: [],
  last_fetched: null,
};
const transitionList: Transition[] = [
  { id: "31", name: "Done", to_status: "Done", to_category: "done" },
];
const status: TicketStatus = { key: "ABC-1", status: "Done", status_category: "done" };
const ticketDraft: TicketDraft = {
  comment: "Shipped",
  suggested_transition_id: "31",
  transitions: transitionList,
};
const ticketDetail: TicketDetail = {
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  issue_type: "Bug",
  priority: "High",
  assignee: "Tomas",
  updated: "2026-09-20T10:00:00.000+0000",
  url: "https://x.atlassian.net/browse/ABC-1",
  description: "Steps",
  comments: [{ id: "1", author: "Tomas", created: "2026-09-20T10:00:00.000+0000", body: "hi" }],
};
const report: PullReport = {
  monday: "2026-09-21",
  worklogs: 3,
  outside: 1,
  schedule_days: 7,
  pulled_at: "2026-09-24T10:00:00Z",
};
const closeoutBody: WeekCloseout = { monday: "2026-09-21", days: [], pulled_at: null };

const ticketBlocks: TicketBlocks = { key: "ABC-1", from: "2026-09-11", to: "2026-09-24", days: [] };
const rawBlock: RawBlock = {
  id: 7,
  day: "2026-09-24",
  jira_issue: "ABC-1",
  started_at: "2026-09-24T09:00:00Z",
  ended_at: "2026-09-24T09:30:00Z",
  duration_seconds: 1800,
  description: "Did it",
  estimated_by: "manual",
  flagged: false,
  tempo_worklog_id: null,
  is_personal: false,
  dirty: false,
  exported_at: null,
  ignored_at: null,
  ticket_origin: "manual",
};
const blocksImpl = mock(async (_key: string) => ticketBlocks);
const logImpl = mock(async (_key: string, _body: unknown) => rawBlock);
const tasksImpl = mock(async (_monday?: string) => tasksBody);
const transitionsImpl = mock(async (_key: string) => transitionList);
const transitionImpl = mock(async (_key: string, _id: string) => status);
const commentImpl = mock(async (_key: string, _text: string) => ({ ok: true as const }));
const draftImpl = mock(async (_key: string) => ticketDraft);
const detailImpl = mock(async (_key: string) => ticketDetail);
const pullImpl = mock(async (_monday: string) => report);
const closeoutImpl = mock(async (_monday: string) => closeoutBody);

mock.module("@/lib/daemonHub", () => ({
  tasks: (m?: string) => tasksImpl(m),
  transitions: (k: string) => transitionsImpl(k),
  transition: (k: string, id: string) => transitionImpl(k, id),
  comment: (k: string, t: string) => commentImpl(k, t),
  detail: (k: string) => detailImpl(k),
  draft: (k: string) => draftImpl(k),
  pullTempo: (m: string) => pullImpl(m),
  closeout: (m: string) => closeoutImpl(m),
  blocks: (k: string) => blocksImpl(k),
  logTime: (k: string, b: unknown) => logImpl(k, b),
}));

let hub: typeof import("./actions-hub");

beforeAll(async () => {
  hub = await import("./actions-hub");
});

beforeEach(() => {
  revalidateImpl.mockReset();
  for (const m of [
    tasksImpl,
    transitionsImpl,
    transitionImpl,
    commentImpl,
    draftImpl,
    detailImpl,
    pullImpl,
    closeoutImpl,
    blocksImpl,
    logImpl,
  ]) {
    m.mockClear();
  }
});

describe("reads", () => {
  it("loadTasks returns the daemon body without revalidating", async () => {
    expect(await hub.loadTasks("2026-09-21")).toEqual({ ok: true, data: tasksBody });
    expect(tasksImpl).toHaveBeenCalledWith("2026-09-21");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("loadTransitions returns the list", async () => {
    expect(await hub.loadTransitions("ABC-1")).toEqual({ ok: true, data: transitionList });
  });

  it("loadTicketDetail returns the detail without revalidating", async () => {
    expect(await hub.loadTicketDetail("ABC-1")).toEqual({ ok: true, data: ticketDetail });
    expect(detailImpl).toHaveBeenCalledWith("ABC-1");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("loadTicketBlocks returns the blocks without revalidating", async () => {
    expect(await hub.loadTicketBlocks("ABC-1")).toEqual({ ok: true, data: ticketBlocks });
    expect(blocksImpl).toHaveBeenCalledWith("ABC-1");
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("draftTicketUpdate returns the draft", async () => {
    expect(await hub.draftTicketUpdate("ABC-1")).toEqual({ ok: true, data: ticketDraft });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("loadCloseout returns the week", async () => {
    expect(await hub.loadCloseout("2026-09-21")).toEqual({ ok: true, data: closeoutBody });
  });

  it("surfaces a daemon error as ok:false", async () => {
    tasksImpl.mockImplementationOnce(async () => {
      throw new Error("jira down");
    });
    expect(await hub.loadTasks()).toEqual({ ok: false, error: "jira down" });
  });
});

describe("writes", () => {
  it("transitionTicket returns the new status and revalidates /tasks", async () => {
    expect(await hub.transitionTicket("ABC-1", "31")).toEqual({ ok: true, data: status });
    expect(transitionImpl).toHaveBeenCalledWith("ABC-1", "31");
    expect(revalidateImpl).toHaveBeenCalledWith("/tasks");
  });

  it("commentOnTicket revalidates /tasks", async () => {
    expect(await hub.commentOnTicket("ABC-1", "hi")).toEqual({ ok: true, data: { ok: true } });
    expect(commentImpl).toHaveBeenCalledWith("ABC-1", "hi");
    expect(revalidateImpl).toHaveBeenCalledWith("/tasks");
  });

  it("logTicketTime posts the body and revalidates that day", async () => {
    const body = { day: "2026-09-24", start: "09:00", minutes: 30, description: "Did it" };
    expect(await hub.logTicketTime("ABC-1", body)).toEqual({ ok: true, data: rawBlock });
    expect(logImpl).toHaveBeenCalledWith("ABC-1", body);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("pullTempoWeek revalidates that week's page", async () => {
    expect(await hub.pullTempoWeek("2026-09-21")).toEqual({ ok: true, data: report });
    expect(revalidateImpl).toHaveBeenCalledWith("/week/2026-09-21");
  });

  it("a failing write does not revalidate", async () => {
    transitionImpl.mockImplementationOnce(async () => {
      throw new Error("Jira said 400: bad transition");
    });
    expect(await hub.transitionTicket("ABC-1", "99")).toEqual({
      ok: false,
      error: "Jira said 400: bad transition",
    });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("reports a refresh failure after a successful write", async () => {
    revalidateImpl.mockImplementationOnce(() => {
      throw new Error("no cache");
    });
    expect(await hub.commentOnTicket("ABC-1", "hi")).toEqual({
      ok: false,
      error: "write succeeded but page refresh failed: no cache",
    });
  });
});
