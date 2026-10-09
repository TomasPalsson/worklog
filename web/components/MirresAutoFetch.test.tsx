import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render, waitFor } from "@testing-library/react";

const refresh = mock(() => {});
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh }),
  usePathname: () => "/",
}));
const { MirresAutoFetch, MIRRES_REFRESH_MS } = await import("./MirresAutoFetch");

const ok = () => mock(async (_day: string) => ({ ok: true as const, data: [] }));
const D = "2026-10-06";
const key = (d: string) => `mirres-auto-${d}`;

beforeEach(() => {
  sessionStorage.clear();
  refresh.mockClear();
});
afterEach(cleanup);

describe("MirresAutoFetch", () => {
  it("fetches on mount, refreshes once and stores a timestamp", async () => {
    const fetchDay = ok();
    const before = Date.now();
    render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    expect(fetchDay).toHaveBeenCalledTimes(1);
    expect(fetchDay).toHaveBeenCalledWith(D);
    const stored = Number(sessionStorage.getItem(key(D)));
    expect(stored).toBeGreaterThanOrEqual(before);
    expect(stored).toBeLessThanOrEqual(Date.now());
  });

  it("skips a day fetched less than 5 minutes ago", async () => {
    sessionStorage.setItem(key(D), String(Date.now() - 1000));
    const fetchDay = ok();
    render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await Promise.resolve();
    expect(fetchDay).not.toHaveBeenCalled();
  });

  it("refetches a day fetched more than 5 minutes ago", async () => {
    sessionStorage.setItem(key(D), String(Date.now() - MIRRES_REFRESH_MS - 1000));
    const fetchDay = ok();
    render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    expect(fetchDay).toHaveBeenCalledTimes(1);
  });

  it("treats the legacy '1' value as due", async () => {
    sessionStorage.setItem(key(D), "1");
    const fetchDay = ok();
    render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await waitFor(() => expect(fetchDay).toHaveBeenCalledTimes(1));
  });

  it("fetches several days sequentially and refreshes once", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const fetchDay = mock(async (d: string) => {
      if (d === "a") await gate;
      return { ok: true as const, data: [] };
    });
    render(<MirresAutoFetch days={["a", "b"]} fetchDay={fetchDay} />);
    await waitFor(() => expect(fetchDay).toHaveBeenCalledTimes(1));
    await Promise.resolve();
    expect(fetchDay).toHaveBeenCalledTimes(1);
    release();
    await waitFor(() => expect(fetchDay).toHaveBeenCalledTimes(2));
    expect(fetchDay.mock.calls.map((c) => c[0])).toEqual(["a", "b"]);
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
  });

  it("does not refresh on failure but still sets the key", async () => {
    const fetchDay = mock(async (_d: string) => ({ ok: false as const, error: "not configured" }));
    render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await waitFor(() => expect(fetchDay).toHaveBeenCalledTimes(1));
    await Promise.resolve();
    expect(refresh).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(key(D))).not.toBeNull();
  });

  it("does nothing for an empty days list", async () => {
    const fetchDay = ok();
    render(<MirresAutoFetch days={[]} fetchDay={fetchDay} />);
    await Promise.resolve();
    expect(fetchDay).not.toHaveBeenCalled();
  });

  it("refetches on window focus once the throttle has elapsed", async () => {
    const fetchDay = ok();
    render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    window.dispatchEvent(new Event("focus"));
    await Promise.resolve();
    expect(fetchDay).toHaveBeenCalledTimes(1); // throttled
    sessionStorage.setItem(key(D), String(Date.now() - MIRRES_REFRESH_MS - 1000));
    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(fetchDay).toHaveBeenCalledTimes(2));
  });

  it("removes listeners on unmount", async () => {
    const fetchDay = ok();
    const { unmount } = render(<MirresAutoFetch days={[D]} fetchDay={fetchDay} />);
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    unmount();
    sessionStorage.clear();
    window.dispatchEvent(new Event("focus"));
    await Promise.resolve();
    expect(fetchDay).toHaveBeenCalledTimes(1);
  });
});
