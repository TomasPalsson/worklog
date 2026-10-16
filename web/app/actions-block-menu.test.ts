// Mocks only the daemon transport (`call`) and next/cache, so the real
// lib/daemonBlockMenu path and body are asserted.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";

const revalidateImpl = mock((_path: string) => {});
mock.module("next/cache", () => ({
  revalidatePath: (path: string) => revalidateImpl(path),
}));

const callImpl = mock(async (_m: string, _p: string, _b?: unknown): Promise<unknown> => ({}));
const realDaemon = { ...(await import("@/lib/daemon")) };
mock.module("@/lib/daemon", () => ({
  ...realDaemon,
  call: (m: string, p: string, b?: unknown) => callImpl(m, p, b),
}));

let redescribeBlock: typeof import("./actions-block-menu").redescribeBlock;
let splitBlock: typeof import("./actions-block-menu").splitBlock;
beforeAll(async () => {
  ({ redescribeBlock, splitBlock } = await import("./actions-block-menu"));
});

beforeEach(() => {
  revalidateImpl.mockReset();
  callImpl.mockReset();
  callImpl.mockImplementation(async () => ({}));
});

describe("redescribeBlock", () => {
  it("POSTs {instruction} to the block's redescribe route, returns the result, revalidates the day", async () => {
    callImpl.mockImplementationOnce(async () => ({ block_id: 42, description: "Fixed the login bug" }));
    const result = await redescribeBlock(42, "2026-09-24", "this was the login bug fix");
    expect(result).toEqual({ ok: true, data: { block_id: 42, description: "Fixed the login bug" } });
    expect(callImpl).toHaveBeenCalledWith("POST", "/blocks/42/redescribe", {
      instruction: "this was the login bug fix",
    });
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("passes an Icelandic 500-char instruction through whole (catches byte counting or truncation)", async () => {
    const instruction = "þ".repeat(500);
    await redescribeBlock(1, "2026-09-24", instruction);
    expect(callImpl).toHaveBeenCalledWith("POST", "/blocks/1/redescribe", { instruction });
  });

  it("surfaces the daemon's refusal and does not revalidate (swallowed error)", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("instruction is empty");
    });
    expect(await redescribeBlock(1, "2026-09-24", " ")).toEqual({ ok: false, error: "instruction is empty" });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("falls back to 'unknown error' for an empty message", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("");
    });
    expect(await redescribeBlock(1, "2026-09-24", "x")).toEqual({ ok: false, error: "unknown error" });
  });

  it("reports a failed page refresh after a successful write", async () => {
    revalidateImpl.mockImplementationOnce(() => {
      throw new Error("boom");
    });
    expect(await redescribeBlock(1, "2026-09-24", "x")).toEqual({
      ok: false,
      error: "write succeeded but page refresh failed: boom",
    });
  });

  for (const id of ["1/../x", 1.5, 0, -3, NaN] as unknown as number[]) {
    it(`rejects block id ${String(id)} without calling the daemon (path injection)`, async () => {
      expect(await redescribeBlock(id, "2026-09-24", "x")).toEqual({ ok: false, error: "invalid block id" });
      expect(callImpl).not.toHaveBeenCalled();
    });
  }
});

describe("splitBlock", () => {
  it("POSTs {first_minutes} to the block's split route and revalidates the day", async () => {
    callImpl.mockImplementationOnce(async () => ({ first: { id: 7 }, second: { id: 8 } }));
    const result = await splitBlock(7, "2026-09-24", 45);
    expect(result).toEqual({ ok: true, data: { first: { id: 7 }, second: { id: 8 } } } as never);
    expect(callImpl).toHaveBeenCalledWith("POST", "/blocks/7/split", { first_minutes: 45 });
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("surfaces the daemon's refusal verbatim and does not revalidate (swallowed error)", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("split point 40m must be between 0 and the block's 30m duration");
    });
    expect(await splitBlock(7, "2026-09-24", 40)).toEqual({
      ok: false,
      error: "split point 40m must be between 0 and the block's 30m duration",
    });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("reports a failed page refresh after a successful write", async () => {
    revalidateImpl.mockImplementationOnce(() => {
      throw new Error("boom");
    });
    expect(await splitBlock(7, "2026-09-24", 5)).toEqual({
      ok: false,
      error: "write succeeded but page refresh failed: boom",
    });
  });

  for (const id of ["1/../x", 1.5, 0, -3, NaN] as unknown as number[]) {
    it(`rejects block id ${String(id)} without calling the daemon (path injection)`, async () => {
      expect(await splitBlock(id, "2026-09-24", 5)).toEqual({ ok: false, error: "invalid block id" });
      expect(callImpl).not.toHaveBeenCalled();
    });
  }

  for (const m of [0, -1, 2.5, NaN, "5"] as unknown as number[]) {
    it(`rejects split minutes ${String(m)} without calling the daemon (non-whole or non-positive)`, async () => {
      expect(await splitBlock(7, "2026-09-24", m)).toEqual({ ok: false, error: "invalid split minutes" });
      expect(callImpl).not.toHaveBeenCalled();
    });
  }
});
