// useNoteRegenerate: Regenerate click on a note block. Mocks only the server-action
// boundary and the router; polling is observed through the poll interval being created.

import { afterEach, beforeEach, describe, expect, it, mock, spyOn } from "bun:test";
import { act, cleanup, renderHook } from "@testing-library/react";
import { dismiss, subscribe, type ToastMsg } from "@/lib/toast";
import { NOTE_POLL_MS } from "@/lib/noteBlock";
import type { DescriptionOrigin, NoteFields, RegenerateNoteResult } from "@/lib/noteBlock";
import type { Block } from "@/lib/types";

mock.module("next/navigation", () => ({ useRouter: () => ({ refresh: () => {} }), usePathname: () => "/" }));

type Res = { ok: true; data: RegenerateNoteResult } | { ok: false; error: string };
let next: Res = { ok: true, data: { started: true } };
const regenerateNoteAction = mock(async (_id: number, _day: string, _force: boolean): Promise<Res> => next);
const noteStatusAction = mock(async () => ({ ok: true as const, data: { state: "running" as const } }));
mock.module("@/app/actions-note-block", () => ({
  regenerateNoteAction,
  noteStatusAction,
  addNoteBlock: mock(async () => ({ ok: false as const, error: "unused" })),
}));
const { useNoteRegenerate } = await import("./useNoteRegenerate");

let polls = 0;
const realSet = globalThis.setInterval;
let toasts: ToastMsg[] = [];
let unsubscribe = () => {};

beforeEach(() => {
  polls = 0;
  next = { ok: true, data: { started: true } };
  regenerateNoteAction.mockClear();
  spyOn(globalThis, "setInterval").mockImplementation(((fn: () => void, ms: number, ...rest: unknown[]) => {
    if (ms !== NOTE_POLL_MS) return (realSet as Function)(fn, ms, ...rest);
    polls += 1;
    return 0 as unknown as number;
  }) as unknown as typeof setInterval);
  unsubscribe = subscribe((m) => (toasts = m));
});
afterEach(() => {
  cleanup();
  unsubscribe();
  for (const t of toasts) dismiss(t.id);
  toasts = [];
  (globalThis.setInterval as unknown as { mockRestore(): void }).mockRestore();
});

const block = (origin: DescriptionOrigin | null) =>
  ({ id: 7, description_origin: origin, rough_note: "fixed it" }) as unknown as Block & NoteFields;

const click = async (origin: DescriptionOrigin | null) => {
  const { result } = renderHook(() => useNoteRegenerate(block(origin), "2026-10-08"));
  await act(async () => void result.current());
};

describe("useNoteRegenerate", () => {
  for (const origin of ["note", "ai", null] as const) {
    it(`origin ${origin}: regenerates at once with force=false and starts polling`, async () => {
      await click(origin);
      // catches an unconditional confirm / force=true
      expect(regenerateNoteAction).toHaveBeenCalledWith(7, "2026-10-08", false);
      expect(toasts.some((t) => t.text.includes("Replace your edit?"))).toBe(false);
      expect(polls).toBe(1);
    });
  }

  it("origin hand: asks first, calls nothing until confirmed", async () => {
    await click("hand");
    // catches skipping the confirm
    expect(regenerateNoteAction).not.toHaveBeenCalled();
    expect(polls).toBe(0);
    const ask = toasts.find((t) => t.text.includes("Replace your edit?"));
    expect(ask?.action).toBeDefined();
  });

  it("origin hand: confirming calls with force=true and starts polling", async () => {
    await click("hand");
    const ask = toasts.find((t) => t.text.includes("Replace your edit?"))!;
    await act(async () => void ask.action!.onClick());
    // catches confirming but still sending force=false
    expect(regenerateNoteAction).toHaveBeenCalledWith(7, "2026-10-08", true);
    expect(polls).toBe(1);
  });

  it("an action error is toasted and nothing is polled (catches a swallowed error)", async () => {
    next = { ok: false, error: "daemon down" };
    await click("ai");
    expect(toasts.some((t) => t.tone === "error" && t.text.includes("daemon down"))).toBe(true);
    expect(polls).toBe(0);
  });

  it("started:false surfaces the reason and does not poll (catches tracking regardless)", async () => {
    next = { ok: true, data: { started: false, reason: "already running" } };
    await click("ai");
    expect(toasts.some((t) => t.tone === "error" && t.text.includes("already running"))).toBe(true);
    expect(polls).toBe(0);
  });
});
