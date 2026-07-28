// B31: the settings panel's three billing-cycle-pruner controls
// (enable toggle, cycle start day, close day) render with associated
// labels and saving sends all three values. Mirrors app/actions.test.ts's
// conventions: file-scope mock.module(...), then beforeAll(async () =>
// await import(...)).

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { SettingsUpdate, SettingsView } from "@/lib/types";

const initialView: SettingsView = {
  personal: { work: [], personal: [] },
  secrets: [],
  timezone: "",
  personal_config_path: null,
  prune_enabled: true,
  cycle_start_day: 20,
  close_day: 23,
};

const fetchSettingsImpl = mock(async () => ({ ok: true as const, data: initialView }));
const saveSettingsCalls: SettingsUpdate[] = [];
const saveSettingsImpl = mock(async (update: SettingsUpdate, _day: string) => {
  saveSettingsCalls.push(update);
  return {
    ok: true as const,
    data: { ...initialView, ...update, reclassified: null },
  };
});

// SettingsPanel talks to @/app/actions directly — mock that boundary so
// no real daemon call (or Next.js server-action runtime) is needed.
mock.module("@/app/actions", () => ({
  fetchSettings: () => fetchSettingsImpl(),
  saveSettings: (update: SettingsUpdate, day: string) => saveSettingsImpl(update, day),
}));

let SettingsPanel: (props: { day: string }) => React.JSX.Element;

beforeAll(async () => {
  const mod = await import("./SettingsPanel");
  SettingsPanel = mod.SettingsPanel;
});

afterEach(() => {
  cleanup();
  fetchSettingsImpl.mockClear();
  saveSettingsImpl.mockClear();
  saveSettingsCalls.length = 0;
});

async function openPanel() {
  render(<SettingsPanel day="2026-07-25" />);
  fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
  // The panel loads settings asynchronously on open — wait for a field
  // that only exists once `view` is hydrated.
  await screen.findByLabelText(/cycle start day/i);
}

describe("SettingsPanel pruner controls (B31)", () => {
  it("renders the enable toggle, cycle start day and close day with associated labels and current values", async () => {
    await openPanel();

    const enableToggle = screen.getByLabelText(/enable automatic pruning/i);
    expect(enableToggle).toBeInstanceOf(HTMLInputElement);
    expect((enableToggle as HTMLInputElement).type).toBe("checkbox");
    expect((enableToggle as HTMLInputElement).checked).toBe(true);

    const startInput = screen.getByLabelText(/cycle start day/i);
    expect((startInput as HTMLInputElement).value).toBe("20");

    const closeInput = screen.getByLabelText(/close day/i);
    expect((closeInput as HTMLInputElement).value).toBe("23");
  });

  it("saving after changing all three pruner fields sends all three values", async () => {
    await openPanel();

    fireEvent.click(screen.getByLabelText(/enable automatic pruning/i));
    fireEvent.change(screen.getByLabelText(/cycle start day/i), {
      target: { value: "15" },
    });
    fireEvent.change(screen.getByLabelText(/close day/i), {
      target: { value: "18" },
    });

    fireEvent.click(screen.getByRole("button", { name: /save changes/i }));

    await waitFor(() => expect(saveSettingsCalls.length).toBe(1));
    expect(saveSettingsCalls[0]).toMatchObject({
      prune_enabled: false,
      cycle_start_day: 15,
      close_day: 18,
    });
  });
});
