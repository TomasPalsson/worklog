"use server";

// Block context-menu Server Actions (spec 026).

import { revalidatePath } from "next/cache";
import { redescribe, splitBlock as daemonSplitBlock } from "@/lib/daemonBlockMenu";
import type { RedescribeResult } from "@/lib/redescribe";
import type { Block } from "@/lib/types";
import type { ActionResult } from "./actions";

export async function redescribeBlock(
  blockId: number,
  day: string,
  instruction: string,
): Promise<ActionResult<RedescribeResult>> {
  if (!Number.isInteger(blockId) || blockId <= 0) return { ok: false, error: "invalid block id" };
  let data: RedescribeResult;
  try {
    data = await redescribe(blockId, instruction);
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
  try {
    revalidatePath(`/${day}`);
  } catch (e) {
    return { ok: false, error: `write succeeded but page refresh failed: ${(e as Error).message}` };
  }
  return { ok: true, data };
}

export async function splitBlock(
  blockId: number,
  day: string,
  firstMinutes: number,
): Promise<ActionResult<{ first: Block; second: Block }>> {
  if (!Number.isInteger(blockId) || blockId <= 0) return { ok: false, error: "invalid block id" };
  if (!Number.isInteger(firstMinutes) || firstMinutes <= 0) return { ok: false, error: "invalid split minutes" };
  let data: { first: Block; second: Block };
  try {
    data = await daemonSplitBlock(blockId, firstMinutes);
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
  try {
    revalidatePath(`/${day}`);
  } catch (e) {
    return { ok: false, error: `write succeeded but page refresh failed: ${(e as Error).message}` };
  }
  return { ok: true, data };
}
