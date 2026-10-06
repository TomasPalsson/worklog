// Standup button (spec 018, FR-17, FR-20, FR-22, FR-23, B4) plus the Settings
// channel field's diff logic.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { PostOutcome, StandupDraft } from "@/lib/daily_helpers_contract";
import type { SettingsUpdate, SettingsView } from "@/lib/types";
import { buildSettingsUpdate, formStateFromView } from "@/lib/settingsForm";

type Res<T> = { ok: true; data: T } | { ok: false; error: string };

const draft: StandupDraft = { today: ["GENAI-12 tenant stack"], next: [], blockers: [] };
const draftStandup = mock(async (): Promise<Res<StandupDraft>> => ({ ok: true, data: draft }));
const postStandup = mock(
  async (_text: string): Promise<Res<PostOutcome>> => ({
    ok: true,
    data: { outcome: "posted", permalink: "https://slack.test/p1" },
  }),
);
const standupChannelSet = mock(async (): Promise<Res<boolean>> => ({ ok: true, data: true }));
mock.module("@/lib/daemonStandup", () => ({ draftStandup, postStandup, standupChannelSet }));

const writeText = mock(async (_t: string) => {});
Object.defineProperty(globalThis.navigator, "clipboard", { value: { writeText }, configurable: true });

let StandupButton: typeof import("./StandupButton").StandupButton;
beforeAll(async () => {
  StandupButton = (await import("./StandupButton")).StandupButton;
});
afterEach(() => {
  cleanup();
  for (const m of [draftStandup, postStandup, standupChannelSet, writeText]) m.mockClear();
});

const open = async () => {
  render(<StandupButton />);
  fireEvent.click(screen.getByRole("button", { name: "Draft standup" }));
  return (await screen.findByRole("textbox", { name: "Standup draft" })) as HTMLTextAreaElement;
};
const post = () => screen.getByRole("button", { name: "Post" }) as HTMLButtonElement;

describe("StandupButton", () => {
  it("drafts on click and posts nothing until Post is clicked", async () => {
    const box = await open();
    expect(draftStandup).toHaveBeenCalledTimes(1);
    // catches: auto-posting the draft
    expect(postStandup).toHaveBeenCalledTimes(0);
    // catches: preview not numbered per the team's three questions
    expect(box.value.startsWith("1. What are you working on today?\n• GENAI-12 tenant stack\n")).toBe(true);
    expect(box.value).toContain("2. What is next/coming up?\n• None\n");
    expect(box.value.endsWith("3. Are there any blockers we need to clear?\n• None\n")).toBe(true);
  });

  it("posts the edited text, not the original draft", async () => {
    const box = await open();
    fireEvent.change(box, { target: { value: "my edit" } });
    fireEvent.click(post());
    await screen.findByText(/Posted/);
    // catches: posting the draft captured at open time
    expect(postStandup).toHaveBeenCalledWith("my edit");
    expect(postStandup).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("link", { name: /thread/i }).getAttribute("href")).toBe("https://slack.test/p1");
  });

  it("disables Post for blank text", async () => {
    const box = await open();
    fireEvent.change(box, { target: { value: "  \n " } });
    // catches: letting a blank post reach the daemon
    expect(post().disabled).toBe(true);
  });

  it("disables Post but keeps Copy while no channel is set", async () => {
    standupChannelSet.mockResolvedValueOnce({ ok: true, data: false });
    await open();
    await waitFor(() => expect(post().disabled).toBe(true));
    fireEvent.click(post());
    // catches: Post only styled disabled, or hidden instead of Copy offered
    expect(postStandup).toHaveBeenCalledTimes(0);
    expect(screen.getByRole("button", { name: "Copy" })).toBeTruthy();
    expect(screen.getByRole("status").textContent).toMatch(/channel/i);
  });

  const cases: [string, PostOutcome, RegExp][] = [
    ["thread missing", { outcome: "no_thread", channel: "daily" }, /No Daily thread.*daily/],
    ["channel unset", { outcome: "no_channel" }, /channel/i],
    ["Slack refuses", { outcome: "slack_refused", error: "channel_not_found" }, /channel_not_found/],
  ];
  for (const [name, data, cause] of cases) {
    it(`shows the cause and Copy when ${name}`, async () => {
      postStandup.mockResolvedValueOnce({ ok: true, data });
      await open();
      fireEvent.click(post());
      // catches: swallowing the failure or showing a generic message
      expect((await screen.findByRole("alert")).textContent).toMatch(cause);
      expect(screen.queryByText(/Posted/)).toBeNull();
      expect(screen.getByRole("button", { name: "Copy" })).toBeTruthy();
    });
  }

  it("shows a transport error from Post", async () => {
    postStandup.mockResolvedValueOnce({ ok: false, error: "daemon down" });
    await open();
    fireEvent.click(post());
    // catches: treating a failed call as posted
    expect((await screen.findByRole("alert")).textContent).toContain("daemon down");
  });

  it("copies the edited text", async () => {
    const box = await open();
    fireEvent.change(box, { target: { value: "edited" } });
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    // catches: copying the original draft
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("edited"));
  });

  it("shows the error and no editor when drafting fails", async () => {
    draftStandup.mockResolvedValueOnce({ ok: false, error: "model unavailable" });
    render(<StandupButton />);
    fireEvent.click(screen.getByRole("button", { name: "Draft standup" }));
    // catches: an empty editor that could post blank text
    expect((await screen.findByRole("alert")).textContent).toContain("model unavailable");
    expect(screen.queryByRole("textbox", { name: "Standup draft" })).toBeNull();
  });
});

const view = (daily_channel: string) =>
  ({
    personal: { work: [], personal: [] },
    secrets: [],
    timezone: "UTC",
    personal_config_path: null,
    prune_enabled: false,
    cycle_start_day: 20,
    close_day: 23,
    work_hours: "",
    abstain_margin: 1.5,
    runner_up_ratio: 1.5,
    daily_channel,
    auto_send: false,
  }) as SettingsView;

describe("Settings daily channel", () => {
  it("hydrates, and an untouched form is no update", () => {
    const v = view("daily");
    // catches: field not hydrated from the view
    expect(formStateFromView(v).dailyChannel).toBe("daily");
    // catches: always sending the channel
    expect(buildSettingsUpdate(v, formStateFromView(v))).toBeNull();
  });

  it("sends a trimmed change and an empty string to clear", () => {
    const v = view("daily");
    const f = formStateFromView(v);
    // catches: untrimmed value sent
    expect(buildSettingsUpdate(v, { ...f, dailyChannel: " team-daily " })).toEqual({
      daily_channel: "team-daily",
    } as SettingsUpdate);
    // catches: treating a cleared field as "no change"
    expect(buildSettingsUpdate(v, { ...f, dailyChannel: "" })).toEqual({ daily_channel: "" } as SettingsUpdate);
  });

  it("whitespace-only against unset is no change", () => {
    const v = view("");
    // catches: comparing untrimmed values
    expect(buildSettingsUpdate(v, { ...formStateFromView(v), dailyChannel: "  " })).toBeNull();
  });
});
