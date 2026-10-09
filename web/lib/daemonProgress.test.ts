import { beforeAll, expect, it, mock } from "bun:test";

const calls: Array<[string, string]> = [];
const callImpl = mock(async (method: string, path: string): Promise<unknown> => {
  calls.push([method, path]);
  return { day: "2026-10-09", tickets: [] };
});
// Spread the real module: bun's mock.module is process-wide, so dropping
// DaemonError & co. here would break every later test file.
const realDaemon = { ...(await import("@/lib/daemon")) };
mock.module("@/lib/daemon", () => ({
  ...realDaemon,
  call: (method: string, path: string) => callImpl(method, path),
}));

let getDayProgress: typeof import("./daemonProgress").getDayProgress;

beforeAll(async () => {
  getDayProgress = (await import("./daemonProgress")).getDayProgress;
});

it("GETs /progress/:day with no query when no ticket is forced", async () => {
  calls.length = 0;
  const r = await getDayProgress("2026-10-09");
  expect(calls).toEqual([["GET", "/progress/2026-10-09"]]); // catches always appending ?refresh=
  expect(r).toEqual({ day: "2026-10-09", tickets: [] });
});

it("adds ?refresh=KEY, URL-encoded, to force one ticket", async () => {
  calls.length = 0;
  await getDayProgress("2026-10-09", "ABC-1");
  await getDayProgress("2026-10-09", "A B&c=1");
  expect(calls.map(([, p]) => p)).toEqual([
    "/progress/2026-10-09?refresh=ABC-1",
    "/progress/2026-10-09?refresh=A%20B%26c%3D1", // catches unencoded interpolation
  ]);
});
