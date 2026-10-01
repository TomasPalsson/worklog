// Tests for the Tempo hub Server Actions (actions-hub.ts).
// Mirrors actions-tempo-lines.test.ts: mock next/cache and the daemon client.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import type {
  PullReport,
  TasksResponse,
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
const report: PullReport = {
  monday: "2026-09-21",
  worklogs: 3,
  outside: 1,
  schedule_days: 7,
  pulled_at: "2026-09-24T10:00:00Z",
};
const closeoutBody: WeekCloseout = { monday: "2026-09-21", days: [], pulled_at: null };

const tasksImpl = mock(async (_monday?: string) => tasksBody);
const transitionsImpl = mock(async (_key: string) => transitionList);
const transitionImpl = mock(async (_key: string, _id: string) => status);
const commentImpl = mock(async (_key: string, _text: string) => ({ ok: true as const }));
const draftImpl = mock(async (_key: string) => ticketDraft);
const pullImpl = mock(async (_monday: string) => report);
const closeoutImpl = mock(async (_monday: string) => closeoutBody);

mock.module("@/lib/daemonHub", () => ({
  tasks: (m?: string) => tasksImpl(m),
  transitions: (k: string) => transitionsImpl(k),
  transition: (k: string, id: string) => transitionImpl(k, id),
  comment: (k: string, t: string) => commentImpl(k, t),
  draft: (k: string) => draftImpl(k),
  pullTempo: (m: string) => pullImpl(m),
  closeout: (m: string) => closeoutImpl(m),
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
    pullImpl,
    closeoutImpl,
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
