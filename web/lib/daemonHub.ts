// Daemon client fns for the Tempo hub (spec 012) — shares lib/daemon.ts's
// `call` transport.

import { call } from "./daemon";
import type {
  CommentBody,
  PullReport,
  TasksResponse,
  TicketDetail,
  TicketDraft,
  TicketStatus,
  Transition,
  TransitionBody,
  WeekCloseout,
} from "./types";

const ticketPath = (key: string, verb: string) =>
  `/tickets/${encodeURIComponent(key)}/${verb}`;

/** No `monday` = the daemon's current week. */
export async function tasks(monday?: string): Promise<TasksResponse> {
  const query = monday ? `?monday=${encodeURIComponent(monday)}` : "";
  return call("GET", `/tasks${query}`);
}

export async function transitions(key: string): Promise<Transition[]> {
  return call("GET", ticketPath(key, "transitions"));
}

export async function detail(key: string): Promise<TicketDetail> {
  return call("GET", ticketPath(key, "detail"));
}

export async function transition(key: string, transitionId: string): Promise<TicketStatus> {
  const body: TransitionBody = { transition_id: transitionId };
  return call("POST", ticketPath(key, "transition"), body);
}

export async function comment(key: string, text: string): Promise<{ ok: true }> {
  const body: CommentBody = { text };
  return call("POST", ticketPath(key, "comment"), body);
}

export async function draft(key: string): Promise<TicketDraft> {
  return call("POST", ticketPath(key, "draft"));
}

export async function pullTempo(monday: string): Promise<PullReport> {
  return call("POST", "/tempo/pull", { monday });
}

export async function closeout(monday: string): Promise<WeekCloseout> {
  return call("GET", `/weeks/${encodeURIComponent(monday)}/closeout`);
}
