// The copy-key icon: Copied for a moment, or Couldn't copy, then back to Copy.

import { afterEach, describe, expect, it, mock, spyOn } from "bun:test";
import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { actions, open } from "./taskModalTestKit";

afterEach(cleanup);

/** Catches the 1.5s reset timer so the test can fire it by hand. */
function captureReset() {
  const real = globalThis.setTimeout;
  const resets: (() => void)[] = [];
  const spy = spyOn(globalThis, "setTimeout").mockImplementation(((fn: () => void, ms?: number, ...rest: unknown[]) =>
    ms === 1500 ? (resets.push(fn), 0) : (real as (...a: unknown[]) => unknown)(fn, ms, ...rest)) as never);
  return { resets, restore: () => spy.mockRestore() };
}
const copy = () => document.querySelector(".task-crumb .task-icon-btn") as HTMLButtonElement;
const useClipboard = (writeText: (t: string) => Promise<void>) =>
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });

describe("copy key feedback", () => {
  it("swaps to a check that says Copied, then goes back to Copy", async () => {
    useClipboard(mock(async () => {}));
    const timer = captureReset();
    try {
      open(actions());
      expect(copy().getAttribute("aria-label")).toBe("Copy ABC-1");
      expect(copy().querySelector(".lucide-copy")).toBeTruthy();
      fireEvent.click(copy());
      await waitFor(() => expect(copy().getAttribute("aria-label")).toBe("Copied ABC-1"));
      expect(copy().getAttribute("data-tip")).toBe("Copied");
      expect(copy().querySelector(".lucide-check")).toBeTruthy();
      expect(timer.resets).toHaveLength(1);
      act(() => timer.resets[0]());
      expect(copy().getAttribute("aria-label")).toBe("Copy ABC-1");
      expect(copy().getAttribute("data-tip")).toBe("Copy key");
      expect(copy().querySelector(".lucide-copy")).toBeTruthy();
    } finally {
      timer.restore();
    }
  });

  it("says Couldn't copy when the browser refuses", async () => {
    useClipboard(mock(async () => Promise.reject(new Error("denied"))));
    open(actions());
    fireEvent.click(copy());
    await waitFor(() => expect(copy().getAttribute("data-tip")).toBe("Couldn't copy"));
    expect(copy().getAttribute("aria-label")).toBe("Couldn't copy ABC-1");
    expect(copy().querySelector(".lucide-check")).toBeNull();
    expect(await screen.findByText("Couldn't copy ABC-1.")).toBeTruthy();
  });

  it("says Couldn't copy when there is no clipboard at all", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
    open(actions());
    fireEvent.click(copy());
    await waitFor(() => expect(copy().getAttribute("data-tip")).toBe("Couldn't copy"));
    expect(screen.queryByText("Copied ABC-1.")).toBeNull();
  });
});
