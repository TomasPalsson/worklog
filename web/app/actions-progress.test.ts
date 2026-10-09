import { afterAll, beforeAll, describe, expect, it } from "bun:test";

import { loadDayProgress } from "./actions-progress";

const realFetch = globalThis.fetch;
const realUrl = process.env.WORKLOG_DAEMON_URL;
let urls: string[] = [];
let respond: () => Response = () =>
  new Response(JSON.stringify({ day: "2026-10-09", tickets: [] }));

beforeAll(() => {
  process.env.WORKLOG_DAEMON_URL = "http://daemon.test";
  globalThis.fetch = (async (u: string) => {
    urls.push(u);
    return respond();
  }) as unknown as typeof fetch;
});
afterAll(() => {
  globalThis.fetch = realFetch;
  if (realUrl === undefined) delete process.env.WORKLOG_DAEMON_URL;
  else process.env.WORKLOG_DAEMON_URL = realUrl;
});

describe("loadDayProgress", () => {
  it("returns the daemon's DayProgress as data", async () => {
    const r = await loadDayProgress("2026-10-09");
    expect(r).toEqual({ ok: true, data: { day: "2026-10-09", tickets: [] } });
    expect(urls.at(-1)).toBe("http://daemon.test/progress/2026-10-09");
  });

  it("forwards the forced-refresh key", async () => {
    await loadDayProgress("2026-10-09", "ABC-1");
    expect(urls.at(-1)).toBe("http://daemon.test/progress/2026-10-09?refresh=ABC-1");
  });

  it("turns a daemon error into ok:false with its message (not a throw, not ok:true)", async () => {
    respond = () => new Response(JSON.stringify({ error: "jira exploded" }), { status: 502 });
    expect(await loadDayProgress("2026-10-09")).toEqual({ ok: false, error: "jira exploded" });
  });

  it("turns an unreachable daemon into ok:false", async () => {
    respond = () => {
      throw new Error("connect refused");
    };
    expect(await loadDayProgress("2026-10-09")).toEqual({ ok: false, error: "connect refused" });
  });
});
