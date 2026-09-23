// B31: the settings panel's three billing-cycle-pruner controls
// (enable toggle, cycle start day, close day) render with associated
// labels and saving sends all three values. Mirrors app/actions.test.ts's
// conventions: file-scope mock.module(...), then beforeAll(async () =>
// await import(...)).

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Rule, RoutingStatus, SettingsUpdate, SettingsView } from "@/lib/types";

const initialView: SettingsView = {
  personal: { work: [], personal: [] },
  secrets: [],
  timezone: "",
  personal_config_path: null,
  prune_enabled: true,
  cycle_start_day: 20,
  close_day: 23,
  work_hours: "Mon-Fri 09:00-17:00",
  route_threshold: 0.9,
};

const initialRules: Rule[] = [];
const initialStatus: RoutingStatus = {
  last_heartbeat: null,
  last_slack: null,
  laya_reachable: false,
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
const fetchRoutingRulesImpl = mock(async () => ({ ok: true as const, data: initialRules }));
const fetchRoutingStatusImpl = mock(async () => ({ ok: true as const, data: initialStatus }));
const deleteRuleCalls: number[] = [];
const deleteRuleImpl = mock(async (id: number, _day: string) => {
  deleteRuleCalls.push(id);
  return { ok: true as const, data: { removed: true } };
});

// SettingsPanel talks to @/app/actions directly — mock that boundary so
// no real daemon call (or Next.js server-action runtime) is needed.
mock.module("@/app/actions", () => ({
  fetchSettings: () => fetchSettingsImpl(),
  saveSettings: (update: SettingsUpdate, day: string) => saveSettingsImpl(update, day),
  fetchRoutingRules: () => fetchRoutingRulesImpl(),
  fetchRoutingStatus: () => fetchRoutingStatusImpl(),
  deleteRule: (id: number, day: string) => deleteRuleImpl(id, day),
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
  fetchRoutingRulesImpl.mockClear();
  fetchRoutingStatusImpl.mockClear();
  deleteRuleImpl.mockClear();
  deleteRuleCalls.length = 0;
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

describe("SettingsPanel browser/Slack routing controls (T012)", () => {
  it("renders work hours and match threshold with current values", async () => {
    await openPanel();

    const workHoursInput = screen.getByLabelText(/work hours/i);
    expect((workHoursInput as HTMLInputElement).value).toBe("Mon-Fri 09:00-17:00");

    const thresholdInput = screen.getByLabelText(/match threshold/i);
    expect((thresholdInput as HTMLInputElement).value).toBe("0.9");
  });

  it("saving after changing work hours and threshold sends both values", async () => {
    await openPanel();

    fireEvent.change(screen.getByLabelText(/work hours/i), {
      target: { value: "Mon-Fri 08:00-18:00" },
    });
    fireEvent.change(screen.getByLabelText(/match threshold/i), {
      target: { value: "0.75" },
    });

    fireEvent.click(screen.getByRole("button", { name: /save changes/i }));

    await waitFor(() => expect(saveSettingsCalls.length).toBe(1));
    expect(saveSettingsCalls[0]).toMatchObject({
      work_hours: "Mon-Fri 08:00-18:00",
      route_threshold: 0.75,
    });
  });

  it("renders the Slack user token as a secret field", async () => {
    fetchSettingsImpl.mockImplementationOnce(async () => ({
      ok: true as const,
      data: {
        ...initialView,
        secrets: [
          { key: "slack_user_token", present: false, sensitive: true, value: null },
        ],
      },
    }));

    await openPanel();

    const tokenInput = screen.getByLabelText(/user token/i);
    expect((tokenInput as HTMLInputElement).type).toBe("password");
  });

  it("shows source status (last heartbeat, last Slack collect, model helper)", async () => {
    fetchRoutingStatusImpl.mockImplementationOnce(async () => ({
      ok: true as const,
      data: {
        last_heartbeat: "2026-07-25T10:00:00Z",
        last_slack: null,
        laya_reachable: true,
      },
    }));

    await openPanel();

    await waitFor(() => expect(fetchRoutingStatusImpl).toHaveBeenCalled());
    expect(await screen.findByText(/reachable/i)).toBeTruthy();
  });

  it("lists hard rules and deletes one", async () => {
    fetchRoutingRulesImpl.mockImplementationOnce(async () => ({
      ok: true as const,
      data: [
        {
          id: 7,
          kind: "domain" as const,
          pattern: "aws.tomasari.is",
          folder: "AWS cert",
          created_at: "2026-07-25T10:00:00Z",
        },
      ],
    }));

    await openPanel();

    const deleteBtn = await screen.findByRole("button", {
      name: /delete rule for aws\.tomasari\.is/i,
    });
    fireEvent.click(deleteBtn);

    await waitFor(() => expect(deleteRuleCalls).toEqual([7]));
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: /delete rule for aws\.tomasari\.is/i }),
      ).toBeNull(),
    );
  });
});
