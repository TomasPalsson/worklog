// Checklist in the Send to Tempo confirm (spec 018, FR-12, FR-13).

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { PreflightRow } from "@/lib/daily_helpers_contract";
import type { TicketDay } from "@/lib/types";
import { SyncConfirm } from "./TaskSyncConfirm";

const day = { day: "2026-10-01", line_seconds: 3600, line_text: "x", in_tempo_seconds: null } as TicketDay;
const green: PreflightRow = { check: "ticketed", ok: true, detail: "Every block has a ticket", target: null };
const red: PreflightRow = { check: "line_text", ok: false, detail: "GENAI-12 on 2026-10-01 has no text", target: "GENAI-12" };

const onSend = mock(() => {});
afterEach(() => {
  cleanup();
  onSend.mockClear();
});

const view = (rows?: PreflightRow[]) =>
  render(<SyncConfirm label="Thu 1 Oct" day={day} changed={false} sending={false} rows={rows} onSend={onSend} onCancel={() => {}} />);
const send = () => screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;

describe("SyncConfirm checklist", () => {
  it("lists every row with its detail, so a red row names what is at fault", () => {
    view([green, red]);
    // catches: rendering only the red rows, or a generic message without the detail
    expect(screen.getByText(green.detail)).toBeTruthy();
    expect(screen.getByText(red.detail)).toBeTruthy();
  });

  it("disables Send while a row is red; Send anyway sends", () => {
    view([green, red]);
    // catches: Send left enabled on red
    expect(send().disabled).toBe(true);
    fireEvent.click(send());
    expect(onSend).toHaveBeenCalledTimes(0);
    fireEvent.click(screen.getByRole("button", { name: "Send anyway" }));
    // catches: Send anyway not wired to a send
    expect(onSend).toHaveBeenCalledTimes(1);
  });

  it("keeps Send enabled and hides Send anyway when every row is green", () => {
    view([green]);
    // catches: disabling whenever rows exist instead of when one is red
    expect(send().disabled).toBe(false);
    expect(screen.queryByRole("button", { name: "Send anyway" })).toBeNull();
    fireEvent.click(send());
    expect(onSend).toHaveBeenCalledTimes(1);
  });

  it("behaves as before with no checklist", () => {
    view(undefined);
    // catches: treating a missing checklist as red
    expect(send().disabled).toBe(false);
    expect(screen.queryByRole("button", { name: "Send anyway" })).toBeNull();
  });
});
