"use server";

import { revalidatePath } from "next/cache";
import {
  assignTicket as daemonAssignTicket,
  setDuration as daemonSetDuration,
  setDescription as daemonSetDescription,
  deleteBlock as daemonDeleteBlock,
  runInfer as daemonRunInfer,
  runEstimate as daemonRunEstimate,
  runSync as daemonRunSync,
  refreshJira as daemonRefreshJira,
} from "@/lib/daemon";

/**
 * Every Server Action returns one of these. `useTransition`'s `start()`
 * swallows thrown errors, so throwing from a Server Action silently
 * leaves the UI in a "success" state. We return a tagged result instead
 * and make the caller handle both branches explicitly via the `toast`.
 */
export type ActionResult<T = undefined> =
  | { ok: true; data: T }
  | { ok: false; error: string };

async function guard<T>(fn: () => Promise<T>): Promise<ActionResult<T>> {
  try {
    return { ok: true, data: await fn() };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

export async function assignTicket(
  blockId: number,
  key: string | null,
  day: string,
): Promise<ActionResult> {
  const r = await guard(() => daemonAssignTicket(blockId, key));
  if (r.ok) revalidatePath(`/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setDuration(
  blockId: number,
  minutes: number,
  day: string,
): Promise<ActionResult> {
  const r = await guard(() => daemonSetDuration(blockId, minutes));
  if (r.ok) revalidatePath(`/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setDescription(
  blockId: number,
  description: string,
  day: string,
): Promise<ActionResult> {
  const r = await guard(() => daemonSetDescription(blockId, description));
  if (r.ok) revalidatePath(`/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function deleteBlock(
  blockId: number,
  day: string,
): Promise<ActionResult> {
  const r = await guard(() => daemonDeleteBlock(blockId));
  if (r.ok) revalidatePath(`/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function runInfer(day: string) {
  const r = await guard(() => daemonRunInfer(day));
  if (r.ok) revalidatePath(`/${day}`);
  return r;
}

export async function runEstimate(day: string) {
  const r = await guard(() => daemonRunEstimate(day));
  if (r.ok) revalidatePath(`/${day}`);
  return r;
}

export async function runSync(day: string, dryRun: boolean) {
  const r = await guard(() => daemonRunSync(day, dryRun));
  if (r.ok) revalidatePath(`/${day}`);
  return r;
}

export async function refreshJira(day: string) {
  const r = await guard(() => daemonRefreshJira());
  if (r.ok) revalidatePath(`/${day}`);
  return r;
}
