"use server";

// Tempo hub Server Actions (spec 012). `run` is duplicated from
// actions-tempo-lines.ts; reads skip revalidation.

import type {
  PullReport,
  TasksResponse,
  TicketDraft,
  TicketStatus,
  Transition,
  WeekCloseout,
} from "@/lib/types";
import type { ActionResult } from "./actions";

const notImplemented = (): never => {
  throw new Error("not implemented");
};

export async function loadTasks(_monday?: string): Promise<ActionResult<TasksResponse>> {
  return notImplemented();
}

export async function loadTransitions(_key: string): Promise<ActionResult<Transition[]>> {
  return notImplemented();
}

export async function transitionTicket(
  _key: string,
  _transitionId: string,
): Promise<ActionResult<TicketStatus>> {
  return notImplemented();
}

export async function commentOnTicket(
  _key: string,
  _text: string,
): Promise<ActionResult<{ ok: true }>> {
  return notImplemented();
}

export async function draftTicketUpdate(_key: string): Promise<ActionResult<TicketDraft>> {
  return notImplemented();
}

export async function pullTempoWeek(_monday: string): Promise<ActionResult<PullReport>> {
  return notImplemented();
}

export async function loadCloseout(_monday: string): Promise<ActionResult<WeekCloseout>> {
  return notImplemented();
}
