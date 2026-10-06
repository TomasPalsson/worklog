import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { RecapBanner } from "./RecapBanner";
import type { Recap } from "@/lib/daily_helpers_contract";

afterEach(cleanup);

const gap = (hour: number, minutes: number) => ({
  started_at: `2026-10-06T${String(hour).padStart(2, "0")}:00:00Z`,
  ended_at: `2026-10-06T${String(hour).padStart(2, "0")}:30:00Z`,
  minutes,
});

const RECAP: Recap = {
  day: "2026-10-06",
  sent: [
    { jira_issue: "AB-1", seconds: 5400 },
    { jira_issue: "AB-2", seconds: 1800 },
  ],
  held_back: [{ jira_issue: "AB-3", reason: "text needs a look" }],
  coverage_percent: 82,
  gaps: [gap(10, 90), gap(13, 45)],
};

const ok = (recap: Recap) => ({ ok: true as const, data: recap });

function setup(recap: Recap | null, resolve = mock(async (..._a: unknown[]) => ok({ ...RECAP, gaps: [] }))) {
  render(<RecapBanner recap={recap} resolve={resolve as never} />);
  return resolve;
}

describe("RecapBanner summary", () => {
  it("renders nothing without a recap", () => {
    const { container } = render(<RecapBanner recap={null} />);
    expect(container.innerHTML).toBe(""); // catches an always-rendered empty shell
  });

  it("shows the sent count with the summed hours", () => {
    setup(RECAP);
    expect(screen.getByText("2 sent · 2.0h")).toBeTruthy(); // catches counting only, or summing seconds wrongly
  });

  it("shows coverage as a whole percent", () => {
    setup(RECAP);
    expect(screen.getByText("82% covered")).toBeTruthy(); // catches a dropped or fractional coverage
  });

  it("lists each held-back line with its reason", () => {
    setup(RECAP);
    const row = screen.getByText("AB-3").closest("li")!;
    expect(within(row).getByText("text needs a look")).toBeTruthy(); // catches a ticket without its reason
  });

  it("has no held-back list when nothing was held back", () => {
    setup({ ...RECAP, held_back: [] });
    expect(screen.queryByText(/held back/i)).toBeNull(); // catches an empty "Held back" heading
  });

  it("has no gap list when there are no gaps", () => {
    setup({ ...RECAP, gaps: [] });
    expect(screen.queryByRole("button", { name: "Personal" })).toBeNull();
  });

  it("shows at most three gaps", () => {
    setup({ ...RECAP, gaps: [gap(9, 90), gap(10, 80), gap(11, 70), gap(12, 60)] });
    expect(screen.getAllByRole("button", { name: "Break" }).length).toBe(3); // catches rendering every gap
  });
});

describe("RecapBanner gap actions", () => {
  it("Personal resolves that gap with the personal action", async () => {
    const resolve = setup(RECAP);
    fireEvent.click(screen.getAllByRole("button", { name: "Personal" })[1]);
    await waitFor(() => expect(resolve).toHaveBeenCalledTimes(1));
    expect(resolve.mock.calls[0]).toEqual(["2026-10-06", RECAP.gaps[1].started_at, { action: "personal" }]); // catches the wrong gap or day
  });

  it("Break resolves with the break action", async () => {
    const resolve = setup(RECAP);
    fireEvent.click(screen.getAllByRole("button", { name: "Break" })[0]);
    await waitFor(() => expect(resolve).toHaveBeenCalledTimes(1));
    expect(resolve.mock.calls[0][2]).toEqual({ action: "break" });
  });

  it("Pick a ticket asks for a key and sends it trimmed", async () => {
    const resolve = setup(RECAP);
    fireEvent.click(screen.getAllByRole("button", { name: "Pick a ticket" })[0]);
    expect(resolve).not.toHaveBeenCalled(); // catches resolving before a key is given
    const save = screen.getByRole("button", { name: "Add time" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true); // catches an empty key being sent
    fireEvent.change(screen.getByLabelText("Ticket key"), { target: { value: " ab-9 " } });
    fireEvent.click(save);
    await waitFor(() => expect(resolve).toHaveBeenCalledTimes(1));
    expect(resolve.mock.calls[0][2]).toEqual({ action: "pick_ticket", jira_issue: "AB-9" }); // catches untrimmed or lowercase key
  });

  it("shows the updated recap after a gap is resolved", async () => {
    setup(RECAP, mock(async () => ok({ ...RECAP, coverage_percent: 90, gaps: [RECAP.gaps[1]] })) as never);
    fireEvent.click(screen.getAllByRole("button", { name: "Break" })[0]);
    await waitFor(() => expect(screen.getByText("90% covered")).toBeTruthy());
    expect(screen.getAllByRole("button", { name: "Break" }).length).toBe(1); // catches keeping the resolved gap
  });

  it("keeps the gap and shows the error when the daemon refuses", async () => {
    setup(RECAP, mock(async () => ({ ok: false as const, error: "no gap starting" })) as never);
    fireEvent.click(screen.getAllByRole("button", { name: "Personal" })[0]);
    await waitFor(() => expect(screen.getByText("no gap starting")).toBeTruthy()); // catches swallowing the error
    expect(screen.getAllByRole("button", { name: "Personal" }).length).toBe(2);
  });
});
