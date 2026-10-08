// Server Actions for note blocks. Mocks only the daemon transport (`call`)
// and next/cache, so the real lib/daemonNoteBlock paths/bodies are asserted.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import type { NoteBlockBody } from "@/lib/noteBlock";
import type { RawBlock } from "@/lib/types";

const revalidateImpl = mock((_path: string) => {});
mock.module("next/cache", () => ({
  revalidatePath: (path: string) => revalidateImpl(path),
}));

const callImpl = mock(async (_m: string, _p: string, _b?: unknown): Promise<unknown> => ({}));
// mock.module is process-wide: keep the real exports so other test files survive.
const realDaemon = { ...(await import("@/lib/daemon")) };
mock.module("@/lib/daemon", () => ({
  ...realDaemon,
  call: (m: string, p: string, b?: unknown) => callImpl(m, p, b),
}));

const body: NoteBlockBody = {
  jira_issue: "ABC-1",
  day: "2026-09-24",
  start: "09:30",
  minutes: 45,
  note: "fixed the thing",
};

let addNoteBlock: typeof import("./actions-note-block").addNoteBlock;
let regenerateNoteAction: typeof import("./actions-note-block").regenerateNoteAction;
let noteStatusAction: typeof import("./actions-note-block").noteStatusAction;

beforeAll(async () => {
  const mod = await import("./actions-note-block");
  addNoteBlock = mod.addNoteBlock;
  regenerateNoteAction = mod.regenerateNoteAction;
  noteStatusAction = mod.noteStatusAction;
});

beforeEach(() => {
  revalidateImpl.mockReset();
  callImpl.mockReset();
  callImpl.mockImplementation(async () => ({}));
});

describe("addNoteBlock", () => {
  it("POSTs the body to /blocks/note, returns the block, revalidates the body's day", async () => {
    callImpl.mockImplementationOnce(async () => ({ id: 7 }));
    const result = await addNoteBlock(body);
    expect(result).toEqual({ ok: true, data: { id: 7 } as unknown as RawBlock });
    expect(callImpl).toHaveBeenCalledWith("POST", "/blocks/note", body);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("surfaces a daemon error and does not revalidate (swallowed error)", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("bad ticket key");
    });
    expect(await addNoteBlock(body)).toEqual({ ok: false, error: "bad ticket key" });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("falls back to 'unknown error' for an empty message", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("");
    });
    expect(await addNoteBlock(body)).toEqual({ ok: false, error: "unknown error" });
  });

  it("reports a failed page refresh after a successful write", async () => {
    revalidateImpl.mockImplementationOnce(() => {
      throw new Error("boom");
    });
    expect(await addNoteBlock(body)).toEqual({
      ok: false,
      error: "write succeeded but page refresh failed: boom",
    });
  });
});

describe("regenerateNoteAction", () => {
  it("POSTs force:false to the block's regenerate route and revalidates the day", async () => {
    callImpl.mockImplementationOnce(async () => ({ started: true }));
    const result = await regenerateNoteAction(12, "2026-09-24", false);
    expect(result).toEqual({ ok: true, data: { started: true } });
    expect(callImpl).toHaveBeenCalledWith("POST", "/blocks/12/note/regenerate", { force: false });
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("sends force:true when forced (a hardcoded false would fail)", async () => {
    await regenerateNoteAction(12, "2026-09-24", true);
    expect(callImpl).toHaveBeenCalledWith("POST", "/blocks/12/note/regenerate", { force: true });
  });

  it("passes through started:false with its reason", async () => {
    callImpl.mockImplementationOnce(async () => ({ started: false, reason: "already running" }));
    expect(await regenerateNoteAction(12, "2026-09-24", false)).toEqual({
      ok: true,
      data: { started: false, reason: "already running" },
    });
  });

  it("surfaces a daemon error and does not revalidate", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("not a note block");
    });
    expect(await regenerateNoteAction(12, "2026-09-24", false)).toEqual({
      ok: false,
      error: "not a note block",
    });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});

describe("noteStatusAction", () => {
  it("GETs the status route and does NOT revalidate (pure poll)", async () => {
    callImpl.mockImplementationOnce(async () => ({ state: "failed", reason: "hand-edited" }));
    const result = await noteStatusAction(12);
    expect(result).toEqual({ ok: true, data: { state: "failed", reason: "hand-edited" } });
    expect(callImpl).toHaveBeenCalledWith("GET", "/blocks/12/note/status", undefined);
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("surfaces a daemon error", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("down");
    });
    expect(await noteStatusAction(12)).toEqual({ ok: false, error: "down" });
  });
});
