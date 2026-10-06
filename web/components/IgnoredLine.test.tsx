import { afterEach, describe, expect, it, mock, spyOn } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { dismiss, subscribe, type ToastMsg } from "@/lib/toast";
import * as actions from "@/app/actions";
import { IgnoredLine } from "./IgnoredLine";

const blocks = [
  {
    id: 7,
    started_at: "2026-04-18T09:00:00+00:00",
    ended_at: "2026-04-18T09:30:00+00:00",
    description: "Fixed the thing",
  },
  {
    id: 8,
    started_at: "2026-04-18T10:00:00+00:00",
    ended_at: "2026-04-18T10:30:00+00:00",
    description: null,
  },
];

afterEach(() => {
  cleanup();
  subscribe((queued) => queued.forEach((msg) => dismiss(msg.id)))();
});

describe("IgnoredLine", () => {
  it("renders nothing when there are no ignored blocks", () => {
    const { container } = render(<IgnoredLine blocks={[]} day="2026-04-18" />);
    expect(container.innerHTML).toBe("");
  });

  it("shows the count, expands, and lists blocks", () => {
    render(<IgnoredLine blocks={blocks} day="2026-04-18" />);
    expect(screen.getByText("2 ignored blocks")).toBeTruthy();
    expect(screen.queryByText("Fixed the thing")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(screen.getByText("Fixed the thing")).toBeTruthy();
    expect(screen.getByText("No description")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Hide" })).toBeTruthy();
  });

  it("Restore calls the action with the block id, false and the day", async () => {
    const restore = mock(async () => ({ ok: true as const }));
    render(<IgnoredLine blocks={blocks} day="2026-04-18" restore={restore} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[0]);
    await waitFor(() => expect(restore).toHaveBeenCalledWith(7, false, "2026-04-18"));
  });

  const queued = () => {
    let q: ToastMsg[] = [];
    subscribe((m) => (q = m))();
    return q;
  };

  it("a successful Restore toast offers Undo through the journal", async () => {
    const restore = mock(async () => ({ ok: true as const }));
    const undo = spyOn(actions, "undoLastChange").mockResolvedValue({
      ok: true,
      data: { outcome: "restored", change: "ignored" as const, block_ids: [7] },
    });
    render(<IgnoredLine blocks={blocks} day="2026-04-18" restore={restore} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[0]);
    await waitFor(() => expect(queued().some((t) => t.action?.label === "Undo")).toBe(true));
    queued().find((t) => t.action?.label === "Undo")!.action!.onClick();
    await waitFor(() => expect(undo).toHaveBeenCalledWith("2026-04-18"));
    expect(restore).toHaveBeenCalledTimes(1);
    undo.mockRestore();
  });

  it("a failed Restore shows an error and no Undo", async () => {
    const restore = mock(async () => ({ ok: false as const, error: "locked" }));
    render(<IgnoredLine blocks={blocks} day="2026-04-18" restore={restore} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[0]);
    await waitFor(() => expect(queued().some((t) => t.tone === "error")).toBe(true));
    expect(queued().some((t) => t.action?.label === "Undo")).toBe(false);
  });
});
