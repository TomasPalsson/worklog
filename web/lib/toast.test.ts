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

describe("toast.undoable", () => {
  const latest = () => {
    let all: any[] = [];
    subscribe((m) => {
      all = m;
    })();
    return all[all.length - 1];
  };
  const click = async (undo: () => Promise<any>) => {
    toast.undoable("Merged 2 blocks", undo);
    latest().action.onClick();
    await new Promise((r) => setTimeout(r, 0));
  };

  it("shows an Undo action on the confirmation", () => {
    toast.undoable("Merged 2 blocks", async () => ({ ok: true, data: { outcome: "nothing_to_undo" } }));
    const m = latest();
    // catches: confirmation toast with no action attached
    expect(m.text).toBe("Merged 2 blocks");
    expect(m.action.label).toBe("Undo");
  });

  it("runs the undo and confirms a restore", async () => {
    const undo = mock(async () => ({
      ok: true,
      data: { outcome: "restored", change: "merge", block_ids: [1, 2] },
    }));
    await click(undo);
    // catches: button that never calls the server action
    expect(undo).toHaveBeenCalledTimes(1);
    expect(latest()).toMatchObject({ tone: "ok", text: "Undid merge" });
  });

  it("says plainly when nothing is left to undo", async () => {
    await click(async () => ({ ok: true, data: { outcome: "nothing_to_undo" } }));
    // catches: wrong message for the 21st undo
    expect(latest()).toMatchObject({ tone: "ok", text: "Nothing left to undo" });
  });

  it("surfaces the Tempo refusal as an error", async () => {
    await click(async () => ({ ok: true, data: { outcome: "refused_synced", block_id: 7 } }));
    // catches: refusal swallowed or shown with ok tone
    expect(latest()).toMatchObject({
      tone: "error",
      text: "Block 7 was sent to Tempo; undo would desync it",
    });
  });

  it("surfaces a failed action as an error", async () => {
    await click(async () => ({ ok: false, error: "daemon down" }));
    // catches: swallowing the error result
    expect(latest()).toMatchObject({ tone: "error", text: "Undo failed — daemon down" });
  });
});
