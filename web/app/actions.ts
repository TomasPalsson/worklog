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
  listBlockEvents as daemonListBlockEvents,
} from "@/lib/daemon";
import type { Event } from "@/lib/types";

/**
 * Every Server Action returns one of these. `useTransition`'s `start()`
 * swallows thrown errors, so throwing from a Server Action silently
 * leaves the UI in a "success" state. We return a tagged result instead
 * and make the caller handle both branches explicitly via the `toast`.
 */
export type ActionResult<T = undefined> =
  | { ok: true; data: T }
  | { ok: false; error: string };

/**
 * Wrap a daemon call plus its `revalidatePath` side effect so any thrown
 * exception — from either the RPC or from Next.js's cache machinery —
 * is caught and surfaced through the tagged `ActionResult`.
 *
 * `revalidatePath` can throw (misconfigured Next context, invalid path,
 * unavailable cache layer); if we didn't wrap it, a successful daemon
 * write followed by a cache-invalidation error would escape the action
 * and be swallowed by `useTransition`, leaving the UI stale with no
 * toast.
 */
async function runAction<T>(
  fn: () => Promise<T>,
  revalidateOn?: string,
): Promise<ActionResult<T>> {
  try {
    const data = await fn();
    if (revalidateOn !== undefined) {
      try {
        revalidatePath(revalidateOn);
      } catch (e) {
        // Best-effort: the write succeeded, the page just won't
        // auto-refresh. Surface as a "partial" failure so the caller
        // can decide (toast as warning vs error).
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

// CRUD actions — `data` is always void; callers check only `ok`.

export async function assignTicket(
  blockId: number,
  key: string | null,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonAssignTicket(blockId, key), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setDuration(
  blockId: number,
  minutes: number,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonSetDuration(blockId, minutes), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function setDescription(
  blockId: number,
  description: string,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonSetDescription(blockId, description), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

export async function deleteBlock(
  blockId: number,
  day: string,
): Promise<ActionResult> {
  const r = await runAction(() => daemonDeleteBlock(blockId), `/${day}`);
  return r.ok ? { ok: true, data: undefined } : r;
}

// Query-style actions — `data` carries the daemon's response payload.

export async function runInfer(day: string) {
  return runAction(() => daemonRunInfer(day), `/${day}`);
}

export async function runEstimate(day: string) {
  return runAction(() => daemonRunEstimate(day), `/${day}`);
}

export async function runSync(day: string, dryRun: boolean) {
  return runAction(() => daemonRunSync(day, dryRun), `/${day}`);
}

export async function refreshJira(day: string) {
  return runAction(() => daemonRefreshJira(), `/${day}`);
}

/**
 * Fetch the events linked to a block. Lazy — called by the per-block
 * drill-down on first expand. No revalidation side effect because the
 * events don't change in response to UI actions.
 */
export async function fetchBlockEvents(
  blockId: number,
): Promise<ActionResult<Event[]>> {
  return runAction(() => daemonListBlockEvents(blockId));
}

// Exported for tests. Not used by callers — they use the CRUD/query
// wrappers above. Exposing the helper lets us verify the happy path
// AND the throws-inside-fn and throws-inside-revalidate branches.
export { runAction as _runActionForTests };
