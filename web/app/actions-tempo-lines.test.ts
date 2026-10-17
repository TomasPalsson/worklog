// Tests for the Tempo ticket-line Server Actions (actions-tempo-lines.ts).
// Mirrors actions-overlaps.test.ts: mock next/cache and the daemon client.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import type { RedescribeTempoLineBody, TempoLine, TempoLineKey } from "@/lib/tempo_line_contract";

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
  billing: null,
};

const setTextImpl = mock(async (_key: TempoLineKey, _text: string) => line);
const setHoursImpl = mock(async (_key: TempoLineKey, _seconds: number | null) => line);
const regenerateImpl = mock(async (_key: TempoLineKey) => line);
const refreshImpl = mock(async (_day: string) => [line]);
const redescribeImpl = mock(async (_body: RedescribeTempoLineBody) => line);

mock.module("@/lib/daemonTempoLines", () => ({
  setTempoLineText: (k: TempoLineKey, t: string) => setTextImpl(k, t),
  setTempoLineHours: (k: TempoLineKey, s: number | null) => setHoursImpl(k, s),
  regenerateTempoLine: (k: TempoLineKey) => regenerateImpl(k),
  refreshMirres: (d: string) => refreshImpl(d),
  redescribeTempoLine: (b: RedescribeTempoLineBody) => redescribeImpl(b),
}));

let saveTempoLineText: typeof import("./actions-tempo-lines").saveTempoLineText;
let saveTempoLineHours: typeof import("./actions-tempo-lines").saveTempoLineHours;
let regenerateTempoLineText: typeof import("./actions-tempo-lines").regenerateTempoLineText;
let redescribeTempoLineText: typeof import("./actions-tempo-lines").redescribeTempoLineText;
let refreshMirresAction: typeof import("./actions-tempo-lines").refreshMirresAction;

beforeAll(async () => {
  const mod = await import("./actions-tempo-lines");
  saveTempoLineText = mod.saveTempoLineText;
  saveTempoLineHours = mod.saveTempoLineHours;
  regenerateTempoLineText = mod.regenerateTempoLineText;
  redescribeTempoLineText = mod.redescribeTempoLineText;
  refreshMirresAction = mod.refreshMirresAction;
});

beforeEach(() => {
  revalidateImpl.mockReset();
  setTextImpl.mockClear();
  setHoursImpl.mockClear();
  regenerateImpl.mockClear();
  refreshImpl.mockClear();
  redescribeImpl.mockClear();
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

describe("redescribeTempoLineText", () => {
  it("sends key + instruction as one body, returns the line, revalidates the day", async () => {
    const result = await redescribeTempoLineText(key, "focus on the login fix");
    expect(result).toEqual({ ok: true, data: line });
    // catches a call that drops the instruction or the jira_issue
    expect(redescribeImpl).toHaveBeenCalledWith({ ...key, instruction: "focus on the login fix" });
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("passes a 500-char Icelandic instruction through whole", async () => {
    const instruction = "þ".repeat(500);
    await redescribeTempoLineText(key, instruction);
    // catches byte counting or truncation in the action
    expect(redescribeImpl).toHaveBeenCalledWith({ ...key, instruction });
  });

  it("surfaces a daemon error and does not revalidate", async () => {
    redescribeImpl.mockImplementationOnce(async () => {
      throw new Error("instruction is empty");
    });
    // catches swallowing the error
    expect(await redescribeTempoLineText(key, " ")).toEqual({
      ok: false,
      error: "instruction is empty",
    });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("reports a failed page refresh after a successful write", async () => {
    revalidateImpl.mockImplementationOnce(() => {
      throw new Error("boom");
    });
    expect(await redescribeTempoLineText(key, "x")).toEqual({
      ok: false,
      error: "write succeeded but page refresh failed: boom",
    });
  });
});

describe("refreshMirresAction", () => {
  it("refreshes the day and revalidates", async () => {
    const result = await refreshMirresAction("2026-09-24");
    expect(result.ok).toBe(true);
    expect(refreshImpl).toHaveBeenCalledWith("2026-09-24");
    expect(revalidateImpl).toHaveBeenCalledWith("/2026-09-24");
  });

  it("surfaces a daemon error", async () => {
    refreshImpl.mockImplementationOnce(async () => {
      throw new Error("Mirres not configured");
    });
    expect(await refreshMirresAction("2026-09-24")).toEqual({
      ok: false,
      error: "Mirres not configured",
    });
  });
});
