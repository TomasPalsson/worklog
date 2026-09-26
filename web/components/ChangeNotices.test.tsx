// Journey 4 / FR-10, FR-11, FR-15 (spec 006). `fetchChanges` /
// `fetchUnseenChanges` / `markChangesSeen` are passed as props (see
// ChangeNotices.tsx's header comment) rather than mocked via
// `mock.module("@/app/actions", ...)`, because several *other* test files
// already replace that whole module for the Bun process and a second
// whole-module replacement here would race them.
//
// Bun 1.2.4's `jest.useFakeTimers()` only fakes `Date`/`performance.now` —
// it does not support `advanceTimersByTime` for `setInterval` (verified:
// the runtime `jest` object exposes no such method). So instead this file
// spies on `globalThis.setInterval` directly, captures the callback
// ChangeNotices registers, and invokes it by hand to simulate a poll tick
// with zero real wall-clock delay — same effect as fake timers, without
// relying on a Bun capability that isn't there yet.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ChangeNotices } from "./ChangeNotices";
import { subscribe, type ToastMsg } from "@/lib/toast";
import type { ActionResult } from "@/app/actions";
import type { BlockChange, ChangeFeed } from "@/lib/deildir";

function change(overrides: Partial<BlockChange> & { id: number }): BlockChange {
  return {
    day: "2026-09-25",
    started_at: "2026-09-25T14:00:00Z",
    field: "description",
    old: "old text",
    new: "new text",
    source: "claude",
    batch: `b${overrides.id}`,
    created_at: "2026-09-25T14:05:00Z",
    seen: false,
    ...overrides,
  };
}

function ok<T>(data: T): ActionResult<T> {
  return { ok: true, data };
}

let capturedTick: (() => void) | null = null;

function installIntervalSpy() {
  capturedTick = null;
  const realSetInterval = globalThis.setInterval;
  const realClearInterval = globalThis.clearInterval;
  globalThis.setInterval = ((cb: () => void) => {
    capturedTick = cb;
    return 1 as unknown as ReturnType<typeof setInterval>;
  }) as typeof setInterval;
  globalThis.clearInterval = (() => {}) as typeof clearInterval;
  return () => {
    globalThis.setInterval = realSetInterval;
    globalThis.clearInterval = realClearInterval;
  };
}

/** Fires the captured setInterval callback and flushes the microtask
 * chain it schedules (one `await` inside the poll handler). */
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

async function tick() {
  capturedTick?.();
  await flush();
}

function captureToasts() {
  let latest: ToastMsg[] = [];
  const unsub = subscribe((m) => {
    latest = m;
  });
  const baseline = latest.length;
  return {
    added: () => latest.slice(baseline),
    stop: unsub,
  };
}

let restoreInterval: () => void;

afterEach(() => {
  cleanup();
  restoreInterval?.();
});

describe("ChangeNotices", () => {
  it("shows exactly one toast naming the source and count for a batch (B10)", async () => {
    restoreInterval = installIntervalSpy();
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> =>
      ok({ changes: [], batches: [], cursor: 5 }),
    );
    const changes = Array.from({ length: 20 }, (_, i) => change({ id: i + 6, batch: "batch-1" }));
    const fetchChanges = mock(
      async (): Promise<ActionResult<ChangeFeed>> =>
        ok({
          changes,
          batches: [{ batch: "batch-1", source: "claude", count: 20 }],
          cursor: 25,
        }),
    );
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 0 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );
    await flush();

    const toasts = captureToasts();
    await tick();

    expect(fetchChanges).toHaveBeenCalledWith(5);
    const added = toasts.added();
    expect(added.length).toBe(1);
    expect(added[0].text).toContain("Claude");
    expect(added[0].text).toContain("20");
    toasts.stop();
  });

  it("shows a catch-up chip on mount; opening it marks seen and clears the chip (B11)", async () => {
    restoreInterval = installIntervalSpy();
    const unseen = [
      change({ id: 1, source: "verdict" }),
      change({ id: 2, source: "verdict" }),
      change({ id: 3, source: "verdict" }),
    ];
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> =>
      ok({ changes: unseen, batches: [{ batch: "b", source: "verdict", count: 3 }], cursor: 3 }),
    );
    const fetchChanges = mock(async (): Promise<ActionResult<ChangeFeed>> => ok({ changes: [], batches: [], cursor: 3 }));
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 3 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );

    const chip = await screen.findByText("3 changes since your last visit");
    expect(screen.queryAllByText(/Verdict/).length).toBe(0); // not shown until opened

    fireEvent.click(chip);

    expect(await screen.findAllByText(/Verdict/)).toHaveLength(3);
    expect(markChangesSeen).toHaveBeenCalledWith(3);
    expect(screen.queryByText("3 changes since your last visit")).toBeNull();
  });

  it("a live toast's Show only opens the list — it does not mark the catch-up seen or clear its chip (F5)", async () => {
    restoreInterval = installIntervalSpy();
    const unseen = [
      change({ id: 1, source: "verdict" }),
      change({ id: 2, source: "verdict" }),
      change({ id: 3, source: "verdict" }),
    ];
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> =>
      ok({ changes: unseen, batches: [{ batch: "b", source: "verdict", count: 3 }], cursor: 3 }),
    );
    const fetchChanges = mock(async (): Promise<ActionResult<ChangeFeed>> =>
      ok({
        changes: [change({ id: 4, source: "claude", batch: "live-1" })],
        batches: [{ batch: "live-1", source: "claude", count: 1 }],
        cursor: 4,
      }),
    );
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 0 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );
    await flush();
    expect(screen.getByText("3 changes since your last visit")).not.toBeNull();

    const toasts = captureToasts();
    await tick();
    const added = toasts.added();
    expect(added.length).toBe(1);
    toasts.stop();

    added[0].action?.onClick();

    expect(markChangesSeen).not.toHaveBeenCalled();
    expect(screen.queryByText("3 changes since your last visit")).not.toBeNull();
  });

  it("opening the catch-up chip marks seen up to the chip's own max id (F5)", async () => {
    restoreInterval = installIntervalSpy();
    const unseen = [
      change({ id: 1, source: "verdict" }),
      change({ id: 2, source: "verdict" }),
      change({ id: 3, source: "verdict" }),
    ];
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> =>
      ok({ changes: unseen, batches: [{ batch: "b", source: "verdict", count: 3 }], cursor: 3 }),
    );
    const fetchChanges = mock(async (): Promise<ActionResult<ChangeFeed>> => ok({ changes: [], batches: [], cursor: 3 }));
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 3 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );

    const chip = await screen.findByText("3 changes since your last visit");
    fireEvent.click(chip);

    expect(markChangesSeen).toHaveBeenCalledWith(3);
    expect(screen.queryByText("3 changes since your last visit")).toBeNull();
  });

  it("never toasts a user-source batch, but it stays in the catch-up (B15)", async () => {
    restoreInterval = installIntervalSpy();
    const userChange = change({ id: 9, source: "user", batch: "u1" });
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> =>
      ok({ changes: [userChange], batches: [{ batch: "u1", source: "user", count: 1 }], cursor: 9 }),
    );
    const fetchChanges = mock(
      async (): Promise<ActionResult<ChangeFeed>> =>
        ok({
          changes: [userChange],
          batches: [{ batch: "u1", source: "user", count: 1 }],
          cursor: 9,
        }),
    );
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 0 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );
    const chip = await screen.findByText("1 changes since your last visit");

    const toasts = captureToasts();
    await tick();
    expect(toasts.added().length).toBe(0);
    toasts.stop();

    fireEvent.click(chip);
    expect(await screen.findAllByText(/You/)).toHaveLength(1);
  });

  it("shows no toast on the first poll when nothing was unseen, even with old changes (edge)", async () => {
    restoreInterval = installIntervalSpy();
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> => ok({ changes: [], batches: [], cursor: 0 }));
    const oldChanges = [change({ id: 40, batch: "old" })];
    const fetchChanges = mock(async (after: number): Promise<ActionResult<ChangeFeed>> => {
      if (after === 0) {
        return ok({ changes: oldChanges, batches: [{ batch: "old", source: "claude", count: 1 }], cursor: 50 });
      }
      return ok({ changes: [change({ id: 51, batch: "fresh" })], batches: [{ batch: "fresh", source: "claude", count: 1 }], cursor: 51 });
    });
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 0 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );
    await flush();

    const toasts = captureToasts();
    await tick();
    expect(toasts.added().length).toBe(0);

    // The guard only ever suppresses the poll that started at cursor 0 —
    // the next one (now starting at 50) behaves normally.
    await tick();
    expect(toasts.added().length).toBe(1);
    toasts.stop();
  });

  it("shows nothing on a failed poll and retries on the next interval", async () => {
    restoreInterval = installIntervalSpy();
    const fetchUnseenChanges = mock(async (): Promise<ActionResult<ChangeFeed>> => ok({ changes: [], batches: [], cursor: 5 }));
    let call = 0;
    const fetchChanges = mock(async (after: number): Promise<ActionResult<ChangeFeed>> => {
      call += 1;
      if (call === 1) throw new Error("network down");
      return ok({
        changes: [change({ id: 6, batch: "b" })],
        batches: [{ batch: "b", source: "claude", count: 1 }],
        cursor: 6,
      });
    });
    const markChangesSeen = mock(async (): Promise<ActionResult<{ marked: number }>> => ok({ marked: 0 }));

    render(
      <ChangeNotices
        fetchChanges={fetchChanges}
        fetchUnseenChanges={fetchUnseenChanges}
        markChangesSeen={markChangesSeen}
      />,
    );
    await flush();

    const toasts = captureToasts();
    await tick();
    expect(toasts.added().length).toBe(0);

    await tick();
    expect(toasts.added().length).toBe(1);
    // Both attempts asked for changes after the same cursor — the failed
    // poll never advanced it.
    expect(fetchChanges).toHaveBeenNthCalledWith(1, 5);
    expect(fetchChanges).toHaveBeenNthCalledWith(2, 5);
    toasts.stop();
  });
});
