// B12: given a day with routed events, when the day page loads, unsorted
// events are listed and every routed event shows its source + origin.
// Mirrors SettingsPanel.test.tsx's convention: mock the @/app/actions
// boundary, no real daemon/server-action runtime needed.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { RoutedEvent, RuleKind } from "@/lib/types";

const unsortedFirefox: RoutedEvent = {
  id: 1,
  source: "firefox",
  started_at: "2026-07-25T10:00:00Z",
  title: "AWS Certified Solutions Architect",
  details: "https://aws.tomasari.is/path",
  container: null,
  folder: null,
  label_origin: null,
  label_confidence: null,
};

const unsortedSlack: RoutedEvent = {
  id: 2,
  source: "slack",
  started_at: "2026-07-25T14:10:00Z",
  title: "sjukra",
  details: "here's the status update",
  container: null,
  folder: null,
  label_origin: null,
  label_confidence: null,
};

const ruleLabelled: RoutedEvent = {
  id: 3,
  source: "firefox",
  started_at: "2026-07-25T09:00:00Z",
  title: "AWS Certified Solutions Architect",
  details: "https://aws.tomasari.is/other",
  container: "work",
  folder: "AWS cert",
  label_origin: "rule",
  label_confidence: null,
};

const guessLabelled: RoutedEvent = {
  id: 4,
  source: "slack",
  started_at: "2026-07-25T14:20:00Z",
  title: "sjukra",
  details: "another message",
  container: null,
  folder: "sjukra",
  label_origin: "guess",
  label_confidence: 0.93,
};

const labelEventCalls: Array<[number, string, RuleKind | null, string]> = [];
const labelEventImpl = mock(
  async (id: number, folder: string, always: RuleKind | null, day: string) => {
    labelEventCalls.push([id, folder, always, day]);
    if (folder === "ghost-project") {
      return { ok: false as const, error: "Project no longer exists" };
    }
    return {
      ok: true as const,
      data: { ...unsortedFirefox, id, folder, label_origin: "fix" as const },
    };
  },
);

mock.module("@/app/actions", () => ({
  labelEvent: (id: number, folder: string, always: RuleKind | null, day: string) =>
    labelEventImpl(id, folder, always, day),
}));

let UnsortedList: (props: {
  day: string;
  events: RoutedEvent[];
  folderOptions: string[];
}) => React.JSX.Element | null;

beforeAll(async () => {
  const mod = await import("./UnsortedList");
  UnsortedList = mod.UnsortedList;
});

afterEach(() => {
  cleanup();
  labelEventImpl.mockClear();
  labelEventCalls.length = 0;
});

function rowFor(title: string): HTMLElement {
  const el = screen.getByText(title).closest("li");
  if (!el) throw new Error(`no <li> ancestor for "${title}"`);
  return el as HTMLElement;
}

describe("UnsortedList (B12)", () => {
  it("lists unsorted events with their source, and shows source + origin on every routed event", () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[unsortedFirefox, unsortedSlack, ruleLabelled, guessLabelled]}
        folderOptions={["AWS cert", "sjukra", "lighthouse"]}
      />,
    );

    // Unsorted events are listed, each with its source.
    const firefoxRows = screen.getAllByText("AWS Certified Solutions Architect").map((n) => n.closest("li")!);
    const unsortedFirefoxRow = firefoxRows.find((r) => within(r as HTMLElement).queryByRole("button", { name: /project for/i }));
    expect(unsortedFirefoxRow).toBeTruthy();
    expect(within(unsortedFirefoxRow as HTMLElement).getByText(/firefox/i)).toBeTruthy();

    const slackRows = screen.getAllByText("sjukra").map((n) => n.closest("li")!);
    const unsortedSlackRow = slackRows.find((r) => within(r as HTMLElement).queryByRole("button", { name: /project for/i }));
    expect(unsortedSlackRow).toBeTruthy();
    expect(within(unsortedSlackRow as HTMLElement).getByText(/slack/i)).toBeTruthy();

    // Every routed event — including already-labelled ones — shows its
    // source and label origin.
    const labelledFirefoxRow = firefoxRows.find((r) => within(r as HTMLElement).queryByText(/rule/i));
    expect(labelledFirefoxRow).toBeTruthy();
    expect(within(labelledFirefoxRow as HTMLElement).getByText(/firefox/i)).toBeTruthy();

    const guessRow = slackRows.find((r) => within(r as HTMLElement).queryByText(/93%/));
    expect(guessRow).toBeTruthy();
    expect(within(guessRow as HTMLElement).getByText(/slack/i)).toBeTruthy();
  });

  it("picking a project for an unsorted event without ticking always labels it with no rule kind", async () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[unsortedFirefox]}
        folderOptions={["AWS cert", "sjukra"]}
      />,
    );

    const row = rowFor("AWS Certified Solutions Architect");
    fireEvent.click(within(row).getByRole("button", { name: /project for/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "AWS cert" }));

    await waitFor(() => expect(labelEventCalls.length).toBe(1));
    expect(labelEventCalls[0]).toEqual([1, "AWS cert", null, "2026-07-25"]);
  });

  it('ticking "always" sends the domain rule kind for a firefox event', async () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[unsortedFirefox]}
        folderOptions={["AWS cert"]}
      />,
    );

    const row = rowFor("AWS Certified Solutions Architect");
    fireEvent.click(within(row).getByRole("checkbox"));
    fireEvent.click(within(row).getByRole("button", { name: /project for/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "AWS cert" }));

    await waitFor(() => expect(labelEventCalls.length).toBe(1));
    expect(labelEventCalls[0]).toEqual([1, "AWS cert", "domain", "2026-07-25"]);
  });

  it('ticking "always" sends the slack_channel rule kind for a slack event', async () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[unsortedSlack]}
        folderOptions={["sjukra"]}
      />,
    );

    const row = rowFor("sjukra");
    fireEvent.click(within(row).getByRole("checkbox"));
    fireEvent.click(within(row).getByRole("button", { name: /project for/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "sjukra" }));

    await waitFor(() => expect(labelEventCalls.length).toBe(1));
    expect(labelEventCalls[0]).toEqual([2, "sjukra", "slack_channel", "2026-07-25"]);
  });

  it("shows the inline error and keeps the event unsorted when the project no longer exists", async () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[unsortedFirefox]}
        folderOptions={["ghost-project"]}
      />,
    );

    const row = rowFor("AWS Certified Solutions Architect");
    fireEvent.click(within(row).getByRole("button", { name: /project for/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "ghost-project" }));

    await screen.findByText(/project no longer exists/i);
    // The event stayed unsorted — still rendered with a picker, not a folder label.
    expect(within(row).getByRole("button", { name: /project for/i })).toBeTruthy();
  });
});
