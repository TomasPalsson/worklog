import { beforeAll, describe, expect, it, mock } from "bun:test";

const paths: string[] = [];
let respond: () => unknown = () => ({ day: "2026-10-09", tickets: [] });
const callImpl = mock(async (_method: string, path: string): Promise<unknown> => {
  paths.push(path);
  return respond();
});
// Spread the real module: bun's mock.module is process-wide, so dropping
// DaemonError & co. here would break every later test file.
const realDaemon = { ...(await import("@/lib/daemon")) };
mock.module("@/lib/daemon", () => ({
  ...realDaemon,
  call: (method: string, path: string) => callImpl(method, path),
}));

let loadDayProgress: typeof import("./actions-progress").loadDayProgress;

beforeAll(async () => {
  loadDayProgress = (await import("./actions-progress")).loadDayProgress;
});

describe("loadDayProgress", () => {
  it("returns the daemon's DayProgress as data", async () => {
    const r = await loadDayProgress("2026-10-09");
    expect(r).toEqual({ ok: true, data: { day: "2026-10-09", tickets: [] } });
    expect(paths.at(-1)).toBe("/progress/2026-10-09");
  });

  it("forwards the forced-refresh key", async () => {
    await loadDayProgress("2026-10-09", "ABC-1");
    expect(paths.at(-1)).toBe("/progress/2026-10-09?refresh=ABC-1");
  });

  it("turns a daemon error into ok:false with its message (not a throw, not ok:true)", async () => {
    respond = () => {
      throw new realDaemon.DaemonError("jira exploded", 502);
    };
    expect(await loadDayProgress("2026-10-09")).toEqual({ ok: false, error: "jira exploded" });
  });

  it("turns an unreachable daemon into ok:false", async () => {
    respond = () => {
      throw new Error("connect refused");
    };
    expect(await loadDayProgress("2026-10-09")).toEqual({ ok: false, error: "connect refused" });
  });
});
