import { afterEach, describe, expect, it, mock } from "bun:test";
import { subscribe, toast } from "./toast";

describe("toast bus", () => {
  afterEach(() => {
    // The bus has module-level state; tests rely on auto-dismissal.
  });

  it("delivers ok messages to subscribers", () => {
    let received: unknown[] = [];
    const unsub = subscribe((msgs) => {
      received = msgs;
    });
    toast.ok("saved");
    expect(received.some((m: any) => m.tone === "ok" && m.text === "saved")).toBe(true);
    unsub();
  });

  it("delivers error messages with tone=error", () => {
    let received: any[] = [];
    const unsub = subscribe((msgs) => {
      received = msgs;
    });
    toast.error("boom");
    expect(received.some((m) => m.tone === "error" && m.text === "boom")).toBe(true);
    unsub();
  });

  it("auto-dismisses messages (at least the ok ones) within their TTL", async () => {
    let received: any[] = [];
    const unsub = subscribe((msgs) => {
      received = msgs;
    });
    const len = received.length;
    toast.ok("transient");
    // Just-pushed
    expect(received.length).toBe(len + 1);
    // Wait past the 3.5s TTL. We use a short sleep + rely on the
    // setTimeout scheduled by toast.ok.
    await new Promise((r) => setTimeout(r, 4000));
    expect(received.some((m) => m.text === "transient")).toBe(false);
    unsub();
  }, 10000);

  it("notifies multiple subscribers", () => {
    const a = mock();
    const b = mock();
    const unA = subscribe(a);
    const unB = subscribe(b);
    toast.ok("fanout");
    expect(a).toHaveBeenCalled();
    expect(b).toHaveBeenCalled();
    unA();
    unB();
  });
});
