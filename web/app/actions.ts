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

export async function assignTicket(
  blockId: number,
  key: string | null,
  day: string,
) {
  await daemonAssignTicket(blockId, key);
  revalidatePath(`/${day}`);
}

export async function setDuration(blockId: number, minutes: number, day: string) {
  await daemonSetDuration(blockId, minutes);
  revalidatePath(`/${day}`);
}

export async function setDescription(
  blockId: number,
  description: string,
  day: string,
) {
  await daemonSetDescription(blockId, description);
  revalidatePath(`/${day}`);
}

export async function deleteBlock(blockId: number, day: string) {
  await daemonDeleteBlock(blockId);
  revalidatePath(`/${day}`);
}

export async function runInfer(day: string) {
  const r = await daemonRunInfer(day);
  revalidatePath(`/${day}`);
  return r;
}

export async function runEstimate(day: string) {
  const r = await daemonRunEstimate(day);
  revalidatePath(`/${day}`);
  return r;
}

export async function runSync(day: string, dryRun: boolean) {
  const r = await daemonRunSync(day, dryRun);
  revalidatePath(`/${day}`);
  return r;
}

export async function refreshJira(day: string) {
  const r = await daemonRefreshJira();
  revalidatePath(`/${day}`);
  return r;
}
