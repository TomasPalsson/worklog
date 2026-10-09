import { afterAll, beforeAll, expect, it } from "bun:test";

import { getDayProgress } from "./daemonProgress";

const realFetch = globalThis.fetch;
const realUrl = process.env.WORKLOG_DAEMON_URL;
let urls: string[] = [];

beforeAll(() => {
  process.env.WORKLOG_DAEMON_URL = "http://daemon.test";
  globalThis.fetch = (async (u: string) => {
    urls.push(u);
    return new Response(JSON.stringify({ day: "2026-10-09", tickets: [] }));
  }) as unknown as typeof fetch;
});
afterAll(() => {
  globalThis.fetch = realFetch;
  if (realUrl === undefined) delete process.env.WORKLOG_DAEMON_URL;
  else process.env.WORKLOG_DAEMON_URL = realUrl;
});

it("GETs /progress/:day with no query when no ticket is forced", async () => {
  urls = [];
  const r = await getDayProgress("2026-10-09");
  expect(urls).toEqual(["http://daemon.test/progress/2026-10-09"]); // catches always appending ?refresh=
  expect(r).toEqual({ day: "2026-10-09", tickets: [] });
});

it("adds ?refresh=KEY, URL-encoded, to force one ticket", async () => {
  urls = [];
  await getDayProgress("2026-10-09", "ABC-1");
  await getDayProgress("2026-10-09", "A B&c=1");
  expect(urls).toEqual([
    "http://daemon.test/progress/2026-10-09?refresh=ABC-1",
    "http://daemon.test/progress/2026-10-09?refresh=A%20B%26c%3D1", // catches unencoded interpolation
  ]);
});
