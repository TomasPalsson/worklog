// Tests for the Tempo ticket-line Server Actions (actions-tempo-lines.ts).
// Mirrors actions-overlaps.test.ts: mock next/cache and the daemon client.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import type { TempoLine, TempoLineKey } from "@/lib/tempo_line_contract";

const revalidateImpl = mock((_path: string) => {});
mock.module("next/cache", () => ({
  revalidatePath: (path: string) => revalidateImpl(path),
}));

const key: TempoLineKey = { day: "2026-09-24", jira_issue: "ABC-1" };
const line: TempoLine = {
  ...key,
  text: "Did the thing",
  text_origin: "manual",
  fallback_text: "fallback",
  union_seconds: 3600,
  hours_override_seconds: 5400,
  effective_seconds: 5400,
};

const setTextImpl = mock(async (_key: TempoLineKey, _text: string) => line);
const setHoursImpl = mock(async (_key: TempoLineKey, _seconds: number | null) => line);
const regenerateImpl = mock(async (_key: TempoLineKey) => line);

mock.module("@/lib/daemonTempoLines", () => ({
  setTempoLineText: (k: TempoLineKey, t: string) => setTextImpl(k, t),
  setTempoLineHours: (k: TempoLineKey, s: number | null) => setHoursImpl(k, s),
  regenerateTempoLine: (k: TempoLineKey) => regenerateImpl(k),
}));

let saveTempoLineText: typeof import("./actions-tempo-lines").saveTempoLineText;
let saveTempoLineHours: typeof import("./actions-tempo-lines").saveTempoLineHours;
let regenerateTempoLineText: typeof import("./actions-tempo-lines").regenerateTempoLineText;

beforeAll(async () => {
  const mod = await import("./actions-tempo-lines");
  saveTempoLineText = mod.saveTempoLineText;
  saveTempoLineHours = mod.saveTempoLineHours;
  regenerateTempoLineText = mod.regenerateTempoLineText;
});

beforeEach(() => {
  revalidateImpl.mockReset();
  setTextImpl.mockClear();
  setHoursImpl.mockClear();
  regenerateImpl.mockClear();
});

describe("saveTempoLineText", () => {
  it("returns the updated line and revalidates the day page", async () => {
    const result = await saveTempoLineText(key, "Did the thing");
    expect(result).toEqual({ ok: true, data: line });
    expect(setTextImpl).toHaveBeenCalledWith(key, "Did the thing");
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("surfaces a daemon error and does not revalidate", async () => {
    setTextImpl.mockImplementationOnce(async () => {
      throw new Error("no such line");
    });
    const result = await saveTempoLineText(key, "x");
    expect(result).toEqual({ ok: false, error: "no such line" });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("reports a failed page refresh after a successful write", async () => {
    revalidateImpl.mockImplementationOnce(() => {
      throw new Error("boom");
    });
    const result = await saveTempoLineText(key, "x");
    expect(result).toEqual({
      ok: false,
      error: "write succeeded but page refresh failed: boom",
    });
  });
});

describe("saveTempoLineHours", () => {
  it("passes seconds through and revalidates", async () => {
    const result = await saveTempoLineHours(key, 5400);
    expect(result).toEqual({ ok: true, data: line });
    expect(setHoursImpl).toHaveBeenCalledWith(key, 5400);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("passes null to clear the override", async () => {
    await saveTempoLineHours(key, null);
    expect(setHoursImpl).toHaveBeenCalledWith(key, null);
  });

  it("surfaces an invalid-hours rejection", async () => {
    setHoursImpl.mockImplementationOnce(async () => {
      throw new Error("hours must be a positive multiple of 1800 seconds");
    });
    const result = await saveTempoLineHours(key, 100);
    expect(result).toEqual({
      ok: false,
      error: "hours must be a positive multiple of 1800 seconds",
    });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});

describe("regenerateTempoLineText", () => {
  it("returns the regenerated line and revalidates", async () => {
    const result = await regenerateTempoLineText(key);
    expect(result).toEqual({ ok: true, data: line });
    expect(regenerateImpl).toHaveBeenCalledWith(key);
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });
});
