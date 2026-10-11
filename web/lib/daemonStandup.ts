"use server";

// Server Actions for the standup button (spec 018). Reads and the post go
// to the daemon; nothing here revalidates a page.

import type { ActionResult } from "@/app/actions";
import { call, loadSettings } from "./daemon";
import type { PostOutcome, StandupDraft } from "./daily_helpers_contract";
import type { FocusSummary } from "./time_leaks_contract";
import type { SettingsView } from "./types";

/** `POST /standup/draft`: the answers with yesterday's focus beside them (never in the posted text). */
export type DraftResult = StandupDraft & { focus?: FocusSummary | null };

async function run<T>(fn: () => Promise<T>): Promise<ActionResult<T>> {
  try {
    return { ok: true, data: await fn() };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

export async function draftStandup(): Promise<ActionResult<DraftResult>> {
  return run(() => call("POST", "/standup/draft", {}));
}

export async function postStandup(text: string): Promise<ActionResult<PostOutcome>> {
  return run(() => call("POST", "/standup/post", { text }));
}

export async function standupChannelSet(): Promise<ActionResult<boolean>> {
  return run(async () => {
    const s: SettingsView & { daily_channel?: string } = await loadSettings();
    return (s.daily_channel ?? "").trim() !== "";
  });
}
