// Tests for the Logged Server Actions (actions-logged.ts).
// Mocks next/cache and the daemon transport so paths/methods/bodies are asserted.

import { beforeAll, beforeEach, describe, expect, it, mock } from "bun:test";
import type { LoggedDay, LoggedRange } from "@/lib/logged_contract";

const revalidateImpl = mock((_path: string, _type?: string) => {});
mock.module("next/cache", () => ({
  revalidatePath: (path: string, type?: string) => revalidateImpl(path, type),
}));

const range: LoggedRange = {
  from: "2026-09-21",
  to: "2026-09-27",
  today: "2026-09-24",
  days: [],
  pulled_at: null,
};
const day: LoggedDay = {
  day: "2026-09-24",
  logged_seconds: 0,
  required_seconds: 28800,
  state: "dismissed",
  dismissal_reason: "sick",
  entries: [],
};

const callImpl = mock(
  async (_method: string, _path: string, _body?: unknown): Promise<unknown> => range,
);
// Spread the real module: bun's mock.module is process-wide, so dropping
// DaemonError & co. here would break every later test file.
const realDaemon = { ...(await import("@/lib/daemon")) };
mock.module("@/lib/daemon", () => ({
  ...realDaemon,
  call: (method: string, path: string, body?: unknown) => callImpl(method, path, body),
}));

let logged: typeof import("./actions-logged");

beforeAll(async () => {
  logged = await import("./actions-logged");
});

beforeEach(() => {
  revalidateImpl.mockReset();
  callImpl.mockReset();
  callImpl.mockImplementation(async () => range);
});

describe("loadLogged", () => {
  it("GETs the range and does not revalidate", async () => {
    expect(await logged.loadLogged("2026-09-21", "2026-09-27")).toEqual({ ok: true, data: range });
    expect(callImpl).toHaveBeenCalledWith(
      "GET",
      "/logged?from=2026-09-21&to=2026-09-27",
      undefined,
    );
    expect(revalidateImpl).not.toHaveBeenCalled();
  });

  it("surfaces a daemon error as ok:false", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("daemon down");
    });
    expect(await logged.loadLogged("a", "b")).toEqual({ ok: false, error: "daemon down" });
  });
});

describe("writes", () => {
  it("refreshLogged POSTs from/to and revalidates the layout", async () => {
    expect(await logged.refreshLogged("2026-09-21", "2026-09-27")).toEqual({
      ok: true,
      data: range,
    });
    expect(callImpl).toHaveBeenCalledWith("POST", "/logged/pull", {
      from: "2026-09-21",
      to: "2026-09-27",
    });
    expect(revalidateImpl).toHaveBeenCalledWith("/logged", "layout");
  });

  it("dismissLoggedDay POSTs day and reason and revalidates", async () => {
    callImpl.mockImplementationOnce(async () => day);
    expect(await logged.dismissLoggedDay("2026-09-24", "sick")).toEqual({ ok: true, data: day });
    expect(callImpl).toHaveBeenCalledWith("POST", "/logged/dismiss", {
      day: "2026-09-24",
      reason: "sick",
    });
    expect(revalidateImpl).toHaveBeenCalledWith("/logged", "layout");
  });

  it("undismissLoggedDay POSTs the day and revalidates", async () => {
    callImpl.mockImplementationOnce(async () => day);
    expect(await logged.undismissLoggedDay("2026-09-24")).toEqual({ ok: true, data: day });
    expect(callImpl).toHaveBeenCalledWith("POST", "/logged/undismiss", { day: "2026-09-24" });
    expect(revalidateImpl).toHaveBeenCalledWith("/logged", "layout");
  });

  it("a failing write does not revalidate", async () => {
    callImpl.mockImplementationOnce(async () => {
      throw new Error("bad reason");
    });
    expect(await logged.dismissLoggedDay("2026-09-24", "")).toEqual({
      ok: false,
      error: "bad reason",
    });
    expect(revalidateImpl).not.toHaveBeenCalled();
  });
});
