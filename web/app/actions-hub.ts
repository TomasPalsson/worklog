"use server";

// Tempo hub Server Actions (spec 012). `run` is duplicated from
// actions-tempo-lines.ts; reads skip revalidation.

import { revalidatePath } from "next/cache";
import * as hub from "@/lib/daemonHub";
import type {
  PullReport,
  TasksResponse,
  TicketDetail,
  TicketDraft,
  TicketStatus,
  Transition,
  WeekCloseout,
} from "@/lib/types";
import type { ActionResult } from "./actions";

async function run<T>(fn: () => Promise<T>, revalidateOn?: string): Promise<ActionResult<T>> {
  try {
    const data = await fn();
    if (revalidateOn !== undefined) {
      try {
        revalidatePath(revalidateOn);
      } catch (e) {
        return {
          ok: false,
          error: `write succeeded but page refresh failed: ${(e as Error).message}`,
        };
      }
    }
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

export async function loadTasks(monday?: string): Promise<ActionResult<TasksResponse>> {
  return run(() => hub.tasks(monday));
}

export async function loadTransitions(key: string): Promise<ActionResult<Transition[]>> {
  return run(() => hub.transitions(key));
}

export async function loadTicketDetail(key: string): Promise<ActionResult<TicketDetail>> {
  return run(() => hub.detail(key));
}

export async function transitionTicket(
  key: string,
  transitionId: string,
): Promise<ActionResult<TicketStatus>> {
  return run(() => hub.transition(key, transitionId), "/tasks");
}

export async function commentOnTicket(
  key: string,
  text: string,
): Promise<ActionResult<{ ok: true }>> {
  return run(() => hub.comment(key, text), "/tasks");
}

export async function draftTicketUpdate(key: string): Promise<ActionResult<TicketDraft>> {
  return run(() => hub.draft(key));
}

export async function pullTempoWeek(monday: string): Promise<ActionResult<PullReport>> {
  return run(() => hub.pullTempo(monday), `/week/${monday}`);
}

export async function loadCloseout(monday: string): Promise<ActionResult<WeekCloseout>> {
  return run(() => hub.closeout(monday));
}
