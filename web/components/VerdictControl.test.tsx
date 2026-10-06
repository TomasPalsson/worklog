import { afterEach, describe, expect, it, mock, spyOn } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { VerdictControl } from "./VerdictControl";
import { buildSettingsUpdate, formStateFromView } from "@/lib/settingsForm";
import { toast } from "@/lib/toast";
import type { SettingsView } from "@/lib/types";
import type { VerdictState, VerdictStatus } from "@/lib/verdict_contract";

afterEach(cleanup);

const status = (s: VerdictState, scorecard: string | null = null): VerdictStatus => ({
  ...s,
  unchecked: 0,
  scorecard,
});

type Ok<T> = { ok: true; data: T };
type Err = { ok: false; error: string };

function setup(
  s: VerdictStatus | Err,
  over: {
    setEnabled?: (on: boolean) => Promise<Ok<VerdictState> | Err>;
    retry?: () => Promise<Ok<VerdictState> | Err>;
  } = {},
) {
  const setEnabled = mock(
    over.setEnabled ?? (async () => ({ ok: true as const, data: { state: "running" } as VerdictState })),
  );
  const retry = mock(
    over.retry ?? (async () => ({ ok: true as const, data: { state: "starting" } as VerdictState })),
  );
  const onAutoSend = mock((_on: boolean) => {});
  const fetchStatus = async () => ("ok" in s ? s : { ok: true as const, data: s });
  render(
    <VerdictControl
      day="2026-10-06"
      autoSend={false}
      onAutoSend={onAutoSend}
      fetchStatus={fetchStatus}
      setEnabled={setEnabled}
      retry={retry}
    />,
  );
  return { setEnabled, retry, onAutoSend };
}

const stateLine = async () => (await screen.findByRole("status")).textContent;
const box = () => screen.getByLabelText("Run Verdict") as HTMLInputElement;

describe("VerdictControl state line", () => {
  const copy: [VerdictState, string][] = [
    [{ state: "off" }, "Off"],
    [{ state: "starting" }, "Starting…"],
    [{ state: "running" }, "Running"],
    [{ state: "not_answering" }, "Not answering"],
    [{ state: "needs_uv" }, "Needs uv — install it with brew install uv, then press Retry"],
    [{ state: "stopped", error: "exit 3" }, "Stopped: exit 3"],
  ];
  for (const [s, text] of copy) {
    // catches: a state mapped to a neighbour's copy, or the wrong dash/ellipsis
    it(`${s.state} renders its exact copy`, async () => {
      setup(status(s));
      expect(await stateLine()).toBe(text);
      expect(screen.getByRole("status").getAttribute("data-state")).toBe(s.state);
    });
  }

  it("shows Checking Verdict… while loading", () => {
    render(
      <VerdictControl
        day="d"
        autoSend={false}
        onAutoSend={() => {}}
        fetchStatus={() => new Promise(() => {})}
      />,
    );
    expect(screen.getByText("Checking Verdict…")).toBeTruthy();
  });

  // catches: swallowing the error, or disabling the box on failure
  it("shows the unknown-state message on fetch error and keeps the box enabled", async () => {
    setup({ ok: false, error: "boom" });
    expect(await screen.findByText("Couldn't reach worklog — Verdict's state is unknown.")).toBeTruthy();
    expect(box().disabled).toBe(false);
  });
});

describe("VerdictControl retry", () => {
  // catches: showing Retry for every state
  for (const s of ["off", "starting", "running", "not_answering"] as const) {
    it(`no Retry for ${s}`, async () => {
      setup(status({ state: s }));
      await stateLine();
      expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    });
  }
  // catches: hiding it for one of the two states that need it
  for (const s of [{ state: "stopped", error: "x" }, { state: "needs_uv" }] as VerdictState[]) {
    it(`Retry for ${s.state} calls retry and shows the returned state`, async () => {
      const { retry } = setup(status(s));
      fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
      await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(screen.getByRole("status").getAttribute("data-state")).toBe("starting"));
    });
  }
});

describe("VerdictControl switch", () => {
  // catches: checked derived from running only; not_answering is still switched on
  it("is checked for not_answering and unchecked for off", async () => {
    setup(status({ state: "not_answering" }));
    await stateLine();
    expect(box().checked).toBe(true);
  });

  // catches: sending the old value instead of the new one
  it("turning on calls setEnabled(true) and shows the returned state", async () => {
    const { setEnabled } = setup(status({ state: "off" }));
    await stateLine();
    fireEvent.click(box());
    await waitFor(() => expect(setEnabled).toHaveBeenCalledWith(true));
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("Running"));
  });

  it("turning off calls setEnabled(false)", async () => {
    const { setEnabled } = setup(status({ state: "running" }));
    await stateLine();
    fireEvent.click(box());
    await waitFor(() => expect(setEnabled).toHaveBeenCalledWith(false));
  });

  // catches: leaving the box live so a second click double-posts
  it("is disabled while the toggle is pending", async () => {
    let release: (v: Ok<VerdictState>) => void = () => {};
    setup(status({ state: "off" }), { setEnabled: () => new Promise((r) => (release = r)) });
    await stateLine();
    fireEvent.click(box());
    await waitFor(() => expect(box().disabled).toBe(true));
    release({ ok: true, data: { state: "running" } });
    await waitFor(() => expect(box().disabled).toBe(false));
  });

  // catches: no toast, wrong message, or the box staying flipped
  it("toasts the daemon's message and flips back on error", async () => {
    const err = spyOn(toast, "error").mockImplementation(() => {});
    setup(status({ state: "off" }), { setEnabled: async () => ({ ok: false, error: "no uv" }) });
    await stateLine();
    fireEvent.click(box());
    await waitFor(() => expect(err).toHaveBeenCalledWith("no uv"));
    await waitFor(() => expect(box().checked).toBe(false));
    expect(screen.getByRole("status").textContent).toBe("Off");
    err.mockRestore();
  });
});

describe("VerdictControl scorecard", () => {
  it("shows the last night's check when present", async () => {
    setup(status({ state: "running" }, "9 of 10 right"));
    expect(await screen.findByText("Last night's check: 9 of 10 right")).toBeTruthy();
  });
  // catches: rendering "Last night's check: null"
  it("hides it when null", async () => {
    setup(status({ state: "running" }));
    await stateLine();
    expect(screen.queryByText(/Last night's check/)).toBeNull();
  });
});

describe("auto-send", () => {
  it("checkbox reports the new value to the form", async () => {
    const { onAutoSend } = setup(status({ state: "off" }));
    await stateLine();
    fireEvent.click(screen.getByLabelText("Send ready lines to Tempo at 17:00"));
    expect(onAutoSend).toHaveBeenCalledWith(true);
  });

  const view = (auto_send: boolean): SettingsView => ({
    personal: { work: [], personal: [] },
    secrets: [],
    timezone: "",
    personal_config_path: null,
    prune_enabled: true,
    cycle_start_day: 20,
    close_day: 23,
    work_hours: "Mon-Fri 09:00-17:00",
    abstain_margin: 1.2,
    runner_up_ratio: 1.1,
    auto_send,
  });

  it("hydrates from the view; unchanged diffs to null", () => {
    expect(formStateFromView(view(true)).autoSend).toBe(true);
    expect(formStateFromView(view(false)).autoSend).toBe(false);
    // catches: a missing field hydrating as truthy
    expect(formStateFromView(view(undefined as unknown as boolean)).autoSend).toBe(false);
    expect(buildSettingsUpdate(view(true), formStateFromView(view(true)))).toBeNull();
  });

  // catches: autoSend left out of the nothing-changed check
  it("a flip alone produces an update, in both directions", () => {
    const on = { ...formStateFromView(view(false)), autoSend: true };
    expect(buildSettingsUpdate(view(false), on)).toEqual({ auto_send: true });
    const off = { ...formStateFromView(view(true)), autoSend: false };
    expect(buildSettingsUpdate(view(true), off)).toEqual({ auto_send: false });
  });
});
