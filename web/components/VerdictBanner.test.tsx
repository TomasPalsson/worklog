import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { VerdictBanner } from "./VerdictBanner";
import type { VerdictStatus } from "@/lib/verdict_contract";

afterEach(cleanup);

const st = (s: Record<string, unknown>, unchecked = 3): VerdictStatus =>
  ({ unchecked, scorecard: null, ...s }) as VerdictStatus;

const ok = { ok: true as const, data: { state: "starting" as const } };

describe("VerdictBanner copy", () => {
  it("renders nothing when running", () => {
    const { container } = render(<VerdictBanner status={st({ state: "running" })} />);
    expect(container.innerHTML).toBe("");
  });

  it("off: quiet line with count and Turn on", () => {
    render(<VerdictBanner status={st({ state: "off" })} />);
    const line = screen.getByRole("status");
    expect(line.getAttribute("data-tone")).toBe("quiet");
    expect(line.textContent).toContain("Verdict is off · 3 events not checked");
    expect(screen.getByRole("button", { name: "Turn on" })).toBeTruthy();
  });

  it("starting: quiet, no action, no count", () => {
    render(<VerdictBanner status={st({ state: "starting" })} />);
    const line = screen.getByRole("status");
    expect(line.getAttribute("data-tone")).toBe("quiet");
    expect(line.textContent).toBe("Verdict is starting");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("not_answering: warn with Turn on", () => {
    render(<VerdictBanner status={st({ state: "not_answering" })} />);
    const line = screen.getByRole("status");
    expect(line.getAttribute("data-tone")).toBe("warn");
    expect(line.textContent).toContain("Verdict isn't answering · 3 events not checked");
    expect(screen.getByRole("button", { name: "Turn on" })).toBeTruthy();
  });

  it("needs_uv: warn with Retry", () => {
    render(<VerdictBanner status={st({ state: "needs_uv" })} />);
    const line = screen.getByRole("status");
    expect(line.getAttribute("data-tone")).toBe("warn");
    expect(line.textContent).toContain("Verdict needs uv · 3 events not checked");
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("stopped: shows the error and Retry, no count", () => {
    render(<VerdictBanner status={st({ state: "stopped", error: "boom" })} />);
    const line = screen.getByRole("status");
    expect(line.getAttribute("data-tone")).toBe("warn");
    expect(line.textContent).toContain("Verdict stopped: boom");
    expect(line.textContent).not.toContain("not checked");
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("N=0 drops the count", () => {
    render(<VerdictBanner status={st({ state: "off" }, 0)} />);
    expect(screen.getByRole("status").textContent).toContain("Verdict is off");
    expect(screen.getByRole("status").textContent).not.toContain("·");
  });

  it("N=1 says 1 event", () => {
    render(<VerdictBanner status={st({ state: "off" }, 1)} />);
    expect(screen.getByRole("status").textContent).toContain("1 event not checked");
    expect(screen.getByRole("status").textContent).not.toContain("1 events");
  });
});

describe("VerdictBanner actions", () => {
  it("off Turn on calls setEnabled(true), not retry", async () => {
    const setEnabled = mock(async (_on: boolean) => ok);
    const retry = mock(async () => ok);
    render(<VerdictBanner status={st({ state: "off" })} setEnabled={setEnabled} retry={retry} />);
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(setEnabled).toHaveBeenCalledWith(true));
    expect(retry).not.toHaveBeenCalled();
  });

  it("not_answering Turn on calls retry, not setEnabled", async () => {
    const setEnabled = mock(async (_on: boolean) => ok);
    const retry = mock(async () => ok);
    render(
      <VerdictBanner status={st({ state: "not_answering" })} setEnabled={setEnabled} retry={retry} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    expect(setEnabled).not.toHaveBeenCalled();
  });

  it("stopped Retry calls retry", async () => {
    const retry = mock(async () => ok);
    render(<VerdictBanner status={st({ state: "stopped", error: "x" })} retry={retry} />);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
  });

  it("needs_uv Retry calls retry, not setEnabled", async () => {
    const setEnabled = mock(async (_on: boolean) => ok);
    const retry = mock(async () => ok);
    render(<VerdictBanner status={st({ state: "needs_uv" })} setEnabled={setEnabled} retry={retry} />);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    expect(setEnabled).not.toHaveBeenCalled(); // catches wiring Retry to setEnabled(true)
  });

  it("a failed Turn on shows the message inline", async () => {
    const setEnabled = mock(async (_on: boolean) => ({ ok: false as const, error: "no uv" }));
    render(<VerdictBanner status={st({ state: "off" })} setEnabled={setEnabled} />);
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(await screen.findByText("Couldn't turn Verdict on: no uv")).toBeTruthy();
  });
});
