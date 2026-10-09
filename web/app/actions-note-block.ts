"use server";

// Note-block Server Actions (spec 019). `run` isn't exported so it isn't
// itself treated as an action.

import { revalidatePath } from "next/cache";
import { logNoteBlock, noteStatus, regenerateNote } from "@/lib/daemonNoteBlock";
import type { NoteBlockBody, NoteJobStatus, RegenerateNoteResult } from "@/lib/noteBlock";
import type { RawBlock } from "@/lib/types";
import type { ActionResult } from "./actions";

const validId = (id: unknown) => Number.isInteger(id) && (id as number) > 0;
const BAD_ID = { ok: false as const, error: "invalid block id" };

async function run<T>(fn: () => Promise<T>, day: string): Promise<ActionResult<T>> {
  try {
    const data = await fn();
    try {
      revalidatePath(`/${day}`);
    } catch (e) {
      return {
        ok: false,
        error: `write succeeded but page refresh failed: ${(e as Error).message}`,
      };
    }
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

export async function addNoteBlock(body: NoteBlockBody): Promise<ActionResult<RawBlock>> {
  return run(() => logNoteBlock(body), body.day);
}

export async function regenerateNoteAction(
  id: number,
  day: string,
  force: boolean,
): Promise<ActionResult<RegenerateNoteResult>> {
  if (!validId(id)) return BAD_ID;
  return run(() => regenerateNote(id, force), day);
}

/** Pure poll — no `revalidatePath`; the caller refreshes once the job settles. */
export async function noteStatusAction(id: number): Promise<ActionResult<NoteJobStatus>> {
  if (!validId(id)) return BAD_ID;
  try {
    return { ok: true, data: await noteStatus(id) };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}
