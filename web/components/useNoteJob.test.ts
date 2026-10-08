// useNoteJob: poll loop for note-block AI jobs. Mocks only the server-action
// boundary and the router; the interval is captured and driven by hand (Bun
// 1.2.4 fake timers cannot advance setInterval, see ChangeNotices.test.tsx).

import { afterEach, beforeEach, describe, expect, it, mock, spyOn } from "bun:test";
import { act, cleanup, renderHook } from "@testing-library/react";
import { dismiss, subscribe, type ToastMsg } from "@/lib/toast";
import { NOTE_POLL_MAX_MS, NOTE_POLL_MS } from "@/lib/noteBlock";
import type { NoteJobStatus } from "@/lib/noteBlock";

const refresh = mock(() => {});
mock.module("next/navigation", () => ({ useRouter: () => ({ refresh }), usePathname: () => "/" }));

type Res = { ok: true; data: NoteJobStatus } | { ok: false; error: string };
let next: Res = { ok: true, data: { state: "running" } };
const noteStatusAction = mock(async (_id: number): Promise<Res> => next);
mock.module("@/app/actions-note-block", () => ({ noteStatusAction, addNoteBlock: mock(async () => ({ ok: false as const, error: "unused" })) }));
const { useNoteJob } = await import("./useNoteJob");

interface Handle {
  fn: () => Promise<void>;
  ms: number;
  cleared: boolean;
}
let handles: Handle[] = [];
const realSet = globalThis.setInterval;
const realClear = globalThis.clearInterval;
let toasts: ToastMsg[] = [];
let unsubscribe = () => {};

beforeEach(() => {
  handles = [];
  next = { ok: true, data: { state: "running" } };
  refresh.mockClear();
  noteStatusAction.mockClear();
  spyOn(globalThis, "setInterval").mockImplementation(((fn: () => Promise<void>, ms: number, ...rest: unknown[]) => {
    if (ms !== NOTE_POLL_MS) return (realSet as Function)(fn, ms, ...rest);
    const h = { fn, ms, cleared: false };
    handles.push(h);
    return h as unknown as number;
  }) as unknown as typeof setInterval);
  spyOn(globalThis, "clearInterval").mockImplementation(((h: unknown) => {
    if (handles.includes(h as Handle)) (h as Handle).cleared = true;
    else (realClear as Function)(h);
  }) as unknown as typeof clearInterval);
  unsubscribe = subscribe((m) => (toasts = m));
});
afterEach(() => {
  cleanup();
  unsubscribe();
  for (const t of toasts) dismiss(t.id);
  toasts = [];
  (globalThis.setInterval as unknown as { mockRestore(): void }).mockRestore();
  (globalThis.clearInterval as unknown as { mockRestore(): void }).mockRestore();
});

const tick = (h: Handle) => act(async () => void (await h.fn()));
const start = (id = 7) => {
  const view = renderHook(() => useNoteJob("2026-10-08"));
  act(() => view.result.current.track(id));
  return view;
};

describe("useNoteJob", () => {
  it("polls the tracked id every NOTE_POLL_MS and lists it as running", async () => {
    const { result } = start(7);
    expect(handles).toHaveLength(1);
    expect(handles[0].ms).toBe(NOTE_POLL_MS);
    expect([...result.current.running]).toEqual([7]);
    await tick(handles[0]);
    expect(noteStatusAction).toHaveBeenCalledWith(7);
    // a still-running job must not refresh or settle (catches refresh-on-every-poll)
    expect(refresh).not.toHaveBeenCalled();
    expect([...result.current.running]).toEqual([7]);
  });

  it("done: refreshes once, clears the interval, drops the id", async () => {
    const { result } = start();
    next = { ok: true, data: { state: "done" } };
    await tick(handles[0]);
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(handles[0].cleared).toBe(true);
    expect(result.current.running.size).toBe(0);
  });

  it("failed: error toast carries the reason, no refresh, id dropped", async () => {
    const { result } = start();
    next = { ok: true, data: { state: "failed", reason: "hand-edited" } };
    await tick(handles[0]);
    expect(toasts.filter((t) => t.tone === "error" && t.text.includes("hand-edited"))).toHaveLength(1);
    expect(refresh).not.toHaveBeenCalled();
    expect(handles[0].cleared).toBe(true);
    expect(result.current.running.size).toBe(0);
  });

  it("a poll that errors keeps polling (a swallowed-error stop would clear it)", async () => {
    const { result } = start();
    next = { ok: false, error: "daemon down" };
    await tick(handles[0]);
    expect(handles[0].cleared).toBe(false);
    expect(result.current.running.size).toBe(1);
  });

  it("stops after exactly NOTE_POLL_MAX_MS of polling, not one poll sooner", async () => {
    const { result } = start();
    const polls = NOTE_POLL_MAX_MS / NOTE_POLL_MS;
    for (let i = 1; i < polls; i++) await tick(handles[0]);
    expect(handles[0].cleared).toBe(false); // catches >= off by one early
    expect(result.current.running.size).toBe(1);
    await tick(handles[0]);
    expect(handles[0].cleared).toBe(true); // catches no cap / > off by one late
    expect(result.current.running.size).toBe(0);
    expect(noteStatusAction).toHaveBeenCalledTimes(polls);
  });

  it("tracks two ids independently", async () => {
    const view = renderHook(() => useNoteJob("2026-10-08"));
    act(() => view.result.current.track(1));
    act(() => view.result.current.track(2));
    next = { ok: true, data: { state: "done" } };
    await tick(handles[0]);
    expect([...view.result.current.running]).toEqual([2]);
    expect(handles[1].cleared).toBe(false);
  });

  it("unmount clears the interval", () => {
    const view = start();
    view.unmount();
    expect(handles[0].cleared).toBe(true);
  });

  it("a poll still in flight at unmount does nothing when it lands", async () => {
    let land: (r: Res) => void = () => {};
    noteStatusAction.mockImplementationOnce(() => new Promise<Res>((r) => (land = r)));
    const view = start();
    const pending = handles[0].fn();
    view.unmount();
    land({ ok: true, data: { state: "done" } });
    await pending;
    expect(refresh).not.toHaveBeenCalled();
  });
});
