import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render, waitFor } from "@testing-library/react";

const refresh = mock(() => {});
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh }),
  usePathname: () => "/",
}));
const { MirresAutoFetch } = await import("./MirresAutoFetch");

const ok = () => mock(async (_day: string) => ({ ok: true as const, data: [] }));

beforeEach(() => {
  sessionStorage.clear();
  refresh.mockClear();
});
afterEach(cleanup);

describe("MirresAutoFetch", () => {
  it("fetches once and refreshes the page", async () => {
    const fetchDay = ok();
    render(<MirresAutoFetch day="2026-10-06" needsFetch fetchDay={fetchDay} />);
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    expect(fetchDay).toHaveBeenCalledTimes(1);
    expect(fetchDay).toHaveBeenCalledWith("2026-10-06");
    expect(sessionStorage.getItem("mirres-auto-2026-10-06")).toBe("1");
  });

  it("does not fetch again when the session key is set", async () => {
    sessionStorage.setItem("mirres-auto-2026-10-06", "1");
    const fetchDay = ok();
    render(<MirresAutoFetch day="2026-10-06" needsFetch fetchDay={fetchDay} />);
    await Promise.resolve();
    expect(fetchDay).not.toHaveBeenCalled();
  });

  it("never fetches when needsFetch is false", async () => {
    const fetchDay = ok();
    render(<MirresAutoFetch day="2026-10-06" needsFetch={false} fetchDay={fetchDay} />);
    await Promise.resolve();
    expect(fetchDay).not.toHaveBeenCalled();
  });

  it("stays silent on failure and does not refresh", async () => {
    const fetchDay = mock(async (_d: string) => ({ ok: false as const, error: "not configured" }));
    render(<MirresAutoFetch day="2026-10-06" needsFetch fetchDay={fetchDay} />);
    await waitFor(() => expect(fetchDay).toHaveBeenCalledTimes(1));
    expect(refresh).not.toHaveBeenCalled();
  });
});
