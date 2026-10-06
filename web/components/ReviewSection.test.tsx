import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ReviewSection } from "./ReviewSection";
import type { ReviewLine } from "@/lib/verdict_contract";

afterEach(cleanup);

const line = (o: Record<string, unknown>): ReviewLine =>
  ({ status: "sent", seconds: 5400, text: "Did the thing", ...o }) as ReviewLine;

const L1 = line({ day: "2026-10-05", jira_issue: "AB-1" });
const L2 = line({ day: "2026-10-05", jira_issue: "AB-2", text: "Second" });
const L3 = line({ day: "2026-10-02", jira_issue: "CD-3", text: "Older" });
const ok = { ok: true as const, data: undefined };

function setup(lines: ReviewLine[], extra: Record<string, unknown> = {}) {
  const base = {
    confirm: mock(async (..._a: unknown[]) => ok),
    sync: mock(async (..._a: unknown[]) => ok),
    saveHours: mock(async (..._a: unknown[]) => ok),
    saveText: mock(async (..._a: unknown[]) => ok),
  };
  const fns = { ...base, ...extra } as typeof base;
  const utils = render(<ReviewSection lines={lines} {...(fns as object)} />);
  return { ...utils, ...fns };
}

describe("ReviewSection list", () => {
  it("renders nothing when there are no lines", () => {
    const { container } = setup([]);
    expect(container.innerHTML).toBe(""); // catches an always-rendered empty shell
  });

  it("shows count, hint and decimal hours", () => {
    setup([L1, L2, L3]);
    expect(screen.getByText("Sent to Tempo — check these")).toBeTruthy();
    expect(screen.getByText("3 lines")).toBeTruthy();
    expect(screen.getAllByText("1.5 h").length).toBe(3);
  });

  it("groups by day, newest first, even when given oldest first", () => {
    const { container } = setup([L3, L1, L2]);
    const heads = [...container.querySelectorAll(".review-sec-day h3")].map((h) => h.textContent);
    expect(heads).toEqual(["Mon 5 Oct", "Fri 2 Oct"]); // catches input-order grouping
    expect(screen.getAllByRole("link", { name: "Open day" })[0].getAttribute("href")).toBe("/2026-10-05");
  });

  it("Looks right confirms (day, issue) and removes only that row", async () => {
    const { confirm } = setup([L1, L2]);
    fireEvent.click(within(screen.getByText("AB-1").closest("li")!).getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(confirm).toHaveBeenCalledWith("2026-10-05", "AB-1"));
    await waitFor(() => expect(screen.queryByText("AB-1")).toBeNull());
    expect(screen.getByText("AB-2")).toBeTruthy(); // catches clearing the whole day
  });

  it("section disappears when the last line is confirmed", async () => {
    const { container } = setup([L1]);
    fireEvent.click(screen.getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(container.querySelector("section")).toBeNull());
    expect(container.textContent).toBe("Confirmed AB-1"); // only the live region remains
  });

  it("a failed confirm keeps the row and shows the message", async () => {
    setup([L1], { confirm: mock(async () => ({ ok: false as const, error: "daemon down" })) });
    fireEvent.click(screen.getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(screen.getByText("daemon down")).toBeTruthy());
    expect(screen.getByText("AB-1")).toBeTruthy(); // catches swallowing the error
  });

  it("Confirm all calls with the day only and counts sent lines only", async () => {
    const ns = line({ day: "2026-10-05", jira_issue: "AB-9", status: "not_sent", error: "boom" });
    const { confirm } = setup([L1, L2, ns]);
    fireEvent.click(screen.getByRole("button", { name: "Confirm all 2" }));
    await waitFor(() => expect(confirm).toHaveBeenCalledWith("2026-10-05"));
    expect(confirm.mock.calls[0].length).toBe(1); // catches passing an undefined issue
    await waitFor(() => expect(screen.queryByText("AB-1")).toBeNull());
    expect(screen.getByText("AB-9")).toBeTruthy(); // a not-sent line is not confirmed away
  });
});

describe("ReviewSection not sent", () => {
  const ns = line({ day: "2026-10-05", jira_issue: "AB-9", status: "not_sent", error: "Tempo said no" });

  it("shows the message and Send again, never Looks right", () => {
    setup([ns]);
    expect(screen.getByText("Not sent: Tempo said no")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Send again" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Looks right" })).toBeNull();
  });

  it("Send again runs sync for that issue, not a dry run", async () => {
    const { sync } = setup([ns]);
    fireEvent.click(screen.getByRole("button", { name: "Send again" }));
    await waitFor(() => expect(sync).toHaveBeenCalledWith("2026-10-05", false, "AB-9"));
  });
});

describe("ReviewSection sync body errors", () => {
  const ns = line({ day: "2026-10-05", jira_issue: "AB-9", status: "not_sent", error: "Tempo said no" });
  const failed = { ok: true as const, data: { synced: 0, skipped: 0, errors: ["Tempo 400: bad hours"] } };

  it("Send again with errors[] in an ok body shows the error and stays Not sent", async () => {
    setup([ns], { sync: mock(async () => failed) });
    fireEvent.click(screen.getByRole("button", { name: "Send again" }));
    await waitFor(() => expect(screen.getByText("Tempo 400: bad hours")).toBeTruthy());
    expect(screen.getByText("Not sent: Tempo said no")).toBeTruthy();
  });

  it("a successful Send again turns the row into a sent row", async () => {
    setup([ns], { sync: mock(async () => ({ ok: true as const, data: { synced: 1, skipped: 0, errors: [] } })) });
    fireEvent.click(screen.getByRole("button", { name: "Send again" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Looks right" })).toBeTruthy());
    expect(screen.queryByText(/Not sent:/)).toBeNull();
  });

  it("Save with errors[] in an ok body stays in edit mode and never confirms", async () => {
    const { confirm } = setup([L1], { sync: mock(async () => failed) });
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Better" } });
    fireEvent.click(screen.getByRole("button", { name: "Save to Tempo" }));
    await waitFor(() => expect(screen.getByText("Tempo 400: bad hours")).toBeTruthy());
    expect(confirm).not.toHaveBeenCalled();
  });
});

describe("ReviewSection edit", () => {
  const open = () => fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  const save = () => screen.getByRole("button", { name: "Save to Tempo" }) as HTMLButtonElement;

  it("renders non-quarter-hour seconds at fixed precision, not raw floats", () => {
    // catches seconds / 3600 unformatted (0.3333333333333333)
    setup([line({ day: "2026-10-05", jira_issue: "AB-1", seconds: 1200 })]);
    expect(screen.getByText("0.33 h")).toBeTruthy();
    open();
    expect((screen.getByLabelText("Hours") as HTMLInputElement).value).toBe("0.33");
    expect(save().disabled).toBe(true);
  });

  it("renders 2700 s as 0.75 h", () => {
    setup([line({ day: "2026-10-05", jira_issue: "AB-1", seconds: 2700 })]);
    expect(screen.getByText("0.75 h")).toBeTruthy();
  });

  it("opens labelled fields and disables Save until something changes", () => {
    setup([L1]);
    open();
    expect((screen.getByLabelText("Hours") as HTMLInputElement).value).toBe("1.5");
    expect((screen.getByLabelText("Text") as HTMLTextAreaElement).value).toBe("Did the thing");
    expect(save().disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "New" } });
    expect(save().disabled).toBe(false);
  });

  it("keeps Save disabled below 0.25 hours (exactly 0.25 is allowed)", () => {
    setup([L1]);
    open();
    fireEvent.change(screen.getByLabelText("Hours"), { target: { value: "0.2" } });
    expect(save().disabled).toBe(true); // catches a missing minimum
    fireEvent.change(screen.getByLabelText("Hours"), { target: { value: "0.25" } });
    expect(save().disabled).toBe(false); // catches > instead of >=
  });

  it("Cancel returns to the row without calling anything", () => {
    const { sync } = setup([L1]);
    open();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Looks right" })).toBeTruthy();
    expect(sync).not.toHaveBeenCalled();
  });

  it("Save runs hours, text, then sync (not a create), then confirm, and the row leaves", async () => {
    const order: string[] = [];
    const key = { day: "2026-10-05", jira_issue: "AB-1" };
    const saveHours = mock(async (..._a: unknown[]) => (order.push("hours"), ok));
    const saveText = mock(async (..._a: unknown[]) => (order.push("text"), ok));
    const sync = mock(async (..._a: unknown[]) => (order.push("sync"), ok));
    const confirm = mock(async (..._a: unknown[]) => (order.push("confirm"), ok));
    setup([L1], { saveHours, saveText, sync, confirm });
    open();
    fireEvent.change(screen.getByLabelText("Hours"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Better" } });
    fireEvent.click(save());
    await waitFor(() => expect(confirm).toHaveBeenCalled());
    expect(order).toEqual(["hours", "text", "sync", "confirm"]);
    expect(saveHours).toHaveBeenCalledWith(key, 7200);
    expect(saveText).toHaveBeenCalledWith(key, "Better");
    expect(sync).toHaveBeenCalledWith("2026-10-05", false, "AB-1");
    expect(confirm).toHaveBeenCalledWith("2026-10-05", "AB-1");
    await waitFor(() => expect(screen.queryByText("AB-1")).toBeNull());
  });

  it("only saves the changed field", async () => {
    const { saveHours, saveText, confirm } = setup([L1]);
    open();
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Better" } });
    fireEvent.click(save());
    await waitFor(() => expect(confirm).toHaveBeenCalled());
    expect(saveHours).not.toHaveBeenCalled(); // catches saving every field
    expect(saveText).toHaveBeenCalled();
  });

  it("a sync error keeps edit mode open, shows the message and never confirms", async () => {
    const sync = mock(async () => ({ ok: false as const, error: "Tempo rejected it" }));
    const { confirm } = setup([L1], { sync });
    open();
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Better" } });
    fireEvent.click(save());
    await waitFor(() => expect(screen.getByText("Tempo rejected it")).toBeTruthy());
    expect(screen.getByLabelText("Text")).toBeTruthy();
    expect(confirm).not.toHaveBeenCalled(); // catches confirming after a failed sync
  });

  it("a save error stops before sync", async () => {
    const saveText = mock(async () => ({ ok: false as const, error: "bad text" }));
    const { sync } = setup([L1], { saveText });
    open();
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Better" } });
    fireEvent.click(save());
    await waitFor(() => expect(screen.getByText("bad text")).toBeTruthy());
    expect(sync).not.toHaveBeenCalled();
  });
});

describe("ReviewSection polish", () => {
  const rowOf = (t: string) => within(screen.getByText(t).closest("li")!);
  const open = () => fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  const live = () => document.querySelector('[aria-live="polite"]')!;

  it("Looks right shows Confirming… while pending", async () => {
    let release: (v: typeof ok) => void = () => {};
    setup([L1, L2], { confirm: mock(() => new Promise<typeof ok>((r) => (release = r))) });
    fireEvent.click(rowOf("AB-1").getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(rowOf("AB-1").getByRole("button", { name: "Confirming…" })).toBeTruthy());
    expect(rowOf("AB-1").getByRole("button", { name: "Confirming…" }).querySelector(".spin")).toBeTruthy();
    expect(rowOf("AB-2").getByRole("button", { name: "Looks right" })).toBeTruthy(); // not every row
    release(ok);
  });

  it("Edit focuses the Hours input", () => {
    setup([L1]);
    open();
    expect(document.activeElement).toBe(screen.getByLabelText("Hours")); // catches no focus move
  });

  it("Cancel returns focus to that row's Edit button", () => {
    setup([L1, L2]);
    fireEvent.click(rowOf("AB-2").getByRole("button", { name: "Edit" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(document.activeElement).toBe(rowOf("AB-2").getByRole("button", { name: "Edit" }));
  });

  it("announces a confirmed row in a polite live region", async () => {
    setup([L1, L2]);
    expect(live().textContent).toBe("");
    fireEvent.click(rowOf("AB-1").getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(live().textContent).toBe("Confirmed AB-1"));
  });

  it("announces the last confirmed row after the section empties", async () => {
    setup([L1]);
    fireEvent.click(rowOf("AB-1").getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Looks right" })).toBeNull());
    expect(live().textContent).toBe("Confirmed AB-1");
  });

  it("keeps the same live node when the section empties", async () => {
    setup([L1]);
    const before = live();
    fireEvent.click(rowOf("AB-1").getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Looks right" })).toBeNull());
    expect(live()).toBe(before); // catches a re-inserted (unannounced) live region
  });

  it("announces a saved row", async () => {
    setup([L1, L2]);
    fireEvent.click(rowOf("AB-1").getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Better" } });
    fireEvent.click(screen.getByRole("button", { name: "Save to Tempo" }));
    await waitFor(() => expect(live().textContent).toBe("Saved AB-1 to Tempo"));
  });

  it("hours below 0.25 show a message and aria-invalid; 0.25 and unchanged do not", () => {
    setup([L1]);
    open();
    const h = screen.getByLabelText("Hours");
    expect(screen.queryByText("Hours must be at least 0.25.")).toBeNull(); // unchanged: silent
    expect(h.getAttribute("aria-invalid")).not.toBe("true");
    fireEvent.change(h, { target: { value: "0.2" } });
    expect(screen.getByText("Hours must be at least 0.25.")).toBeTruthy();
    expect(h.getAttribute("aria-invalid")).toBe("true");
    fireEvent.change(h, { target: { value: "0.25" } }); // catches > instead of >=
    expect(screen.queryByText("Hours must be at least 0.25.")).toBeNull();
    expect(h.getAttribute("aria-invalid")).not.toBe("true");
  });
});
