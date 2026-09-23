// SortTray (rewritten UnsortedList): groups unsorted browser/Slack events
// by channel/DM/site, files or dismisses a whole group at once, and tucks
// filed events away under <AutoFiled>. Mirrors SettingsPanel.test.tsx's
// convention: mock the @/app/actions boundary, no real daemon/server-action
// runtime needed.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { RoutedEvent, RuleKind } from "@/lib/types";

const slack1: RoutedEvent = {
  id: 1,
  source: "slack",
  started_at: "2026-07-25T09:00:00Z",
  title: "sjukra",
  details: "first message",
  container: null,
  folder: null,
  label_origin: null,
  label_confidence: null,
};
const slack2: RoutedEvent = {
  id: 2,
  source: "slack",
  started_at: "2026-07-25T14:10:00Z",
  title: "sjukra",
  details: "second message",
  container: null,
  folder: null,
  label_origin: null,
  label_confidence: null,
};
const firefox1: RoutedEvent = {
  id: 3,
  source: "firefox",
  started_at: "2026-07-25T10:00:00Z",
  title: "AWS docs",
  details: "https://aws.tomasari.is/path",
  container: null,
  folder: null,
  label_origin: null,
  label_confidence: null,
};

const ruleLabelled: RoutedEvent = {
  id: 4,
  source: "firefox",
  started_at: "2026-07-25T09:00:00Z",
  title: "AWS Certified Solutions Architect",
  details: "https://aws.tomasari.is/other",
  container: "work",
  folder: "AWS cert",
  label_origin: "rule",
  label_confidence: null,
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
      data: { id, source: "slack", started_at: "x", title: "x", details: null, container: null, folder, label_origin: "fix" as const, label_confidence: null },
    };
  },
);

const dismissEventCalls: Array<[number, RuleKind | null, string]> = [];
const dismissEventImpl = mock(async (id: number, ruleKind: RuleKind | null, day: string) => {
  dismissEventCalls.push([id, ruleKind, day]);
  return {
    ok: true as const,
    data: { id, source: "slack", started_at: "x", title: "x", details: null, container: null, folder: null, label_origin: "dismissed" as const, label_confidence: null },
  };
});

mock.module("@/app/actions", () => ({
  labelEvent: (id: number, folder: string, always: RuleKind | null, day: string) =>
    labelEventImpl(id, folder, always, day),
  dismissEvent: (id: number, ruleKind: RuleKind | null, day: string) =>
    dismissEventImpl(id, ruleKind, day),
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
  dismissEventImpl.mockClear();
  labelEventCalls.length = 0;
  dismissEventCalls.length = 0;
});

function rowFor(groupLabel: string): HTMLElement {
  const el = screen.getByText(groupLabel).closest("li");
  if (!el) throw new Error(`no <li> ancestor for "${groupLabel}"`);
  return el as HTMLElement;
}

describe("SortTray grouping", () => {
  it("groups events by channel/site into one row per group, with a count pill", () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[slack1, slack2, firefox1]}
        folderOptions={["sjukra", "AWS cert"]}
      />,
    );

    // "23 events · 9 groups" style pill.
    expect(screen.getByText(/3 events · 2 groups/)).toBeTruthy();

    const sjukraRow = rowFor("sjukra");
    expect(within(sjukraRow).getByText("2 messages")).toBeTruthy();
    expect(within(sjukraRow).getByText(/09:00–14:10/)).toBeTruthy();

    const awsRow = rowFor("aws.tomasari.is");
    expect(within(awsRow).getByText("1 visit")).toBeTruthy();
  });

  it("sorts groups by count desc", () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[firefox1, slack1, slack2]}
        folderOptions={["sjukra", "AWS cert"]}
      />,
    );
    const names = screen.getAllByText(/sjukra|aws\.tomasari\.is/).map((n) => n.textContent);
    expect(names[0]).toBe("sjukra");
  });

  it("expanding a group's disclosure shows its individual events", async () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[slack1, slack2]}
        folderOptions={["sjukra"]}
      />,
    );
    const row = rowFor("sjukra");
    // The individual events sit inside a native <details> that starts
    // closed — assert on `.open`, not DOM presence (collapsed content is
    // still in the tree, just CSS-hidden by the browser's UA stylesheet).
    const disclosure = row.querySelector("details.sort-row-disclosure") as HTMLDetailsElement;
    expect(disclosure.open).toBe(false);
    fireEvent.click(within(row).getByText("sjukra"));
    expect(disclosure.open).toBe(true);
    // Scope to the expanded list, not the whole row — the row's one-line
    // summary already shows the group's latest preview ("second message").
    const eventsList = disclosure.querySelector(".sort-row-events") as HTMLElement;
    expect(within(eventsList).getByText("first message")).toBeTruthy();
    expect(within(eventsList).getByText("second message")).toBeTruthy();
  });
});

describe("filing a group", () => {
  it("picking a project labels every event in the group, no rule kind by default", async () => {
    render(
      <UnsortedList day="2026-07-25" events={[slack1, slack2]} folderOptions={["sjukra"]} />,
    );
    const row = rowFor("sjukra");
    fireEvent.click(within(row).getByRole("button", { name: /project for sjukra/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "sjukra" }));

    await waitFor(() => expect(labelEventCalls.length).toBe(2));
    expect(labelEventCalls).toEqual([
      [1, "sjukra", null, "2026-07-25"],
      [2, "sjukra", null, "2026-07-25"],
    ]);
  });

  it('ticking "always" sends the rule kind on the first call only', async () => {
    render(
      <UnsortedList day="2026-07-25" events={[slack1, slack2]} folderOptions={["sjukra"]} />,
    );
    const row = rowFor("sjukra");
    fireEvent.click(within(row).getByRole("checkbox"));
    fireEvent.click(within(row).getByRole("button", { name: /project for sjukra/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "sjukra" }));

    await waitFor(() => expect(labelEventCalls.length).toBe(2));
    expect(labelEventCalls).toEqual([
      [1, "sjukra", "slack_channel", "2026-07-25"],
      [2, "sjukra", null, "2026-07-25"],
    ]);
  });

  it("shows an inline error and keeps the group when a project no longer exists", async () => {
    render(
      <UnsortedList day="2026-07-25" events={[slack1]} folderOptions={["ghost-project"]} />,
    );
    const row = rowFor("sjukra");
    fireEvent.click(within(row).getByRole("button", { name: /project for sjukra/i }));
    fireEvent.click(await within(row).findByRole("option", { name: "ghost-project" }));

    await screen.findByText(/project no longer exists/i);
    expect(within(row).getByRole("button", { name: /project for sjukra/i })).toBeTruthy();
  });
});

describe("dismissing a group", () => {
  it('"Not work" dismisses every event in the group with no rule kind by default', async () => {
    render(
      <UnsortedList day="2026-07-25" events={[slack1, slack2]} folderOptions={["sjukra"]} />,
    );
    const row = rowFor("sjukra");
    fireEvent.click(within(row).getByRole("button", { name: /not work/i }));

    await waitFor(() => expect(dismissEventCalls.length).toBe(2));
    expect(dismissEventCalls).toEqual([
      [1, null, "2026-07-25"],
      [2, null, "2026-07-25"],
    ]);
  });

  it('ticking "always" sends the rule kind on the first dismiss call only', async () => {
    render(
      <UnsortedList day="2026-07-25" events={[slack1, slack2]} folderOptions={["sjukra"]} />,
    );
    const row = rowFor("sjukra");
    fireEvent.click(within(row).getByRole("checkbox"));
    fireEvent.click(within(row).getByRole("button", { name: /not work/i }));

    await waitFor(() => expect(dismissEventCalls.length).toBe(2));
    expect(dismissEventCalls).toEqual([
      [1, "slack_channel", "2026-07-25"],
      [2, null, "2026-07-25"],
    ]);
  });
});

describe("Dismiss the rest", () => {
  it("requires an in-place confirm before dismissing every remaining group", async () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[slack1, slack2, firefox1]}
        folderOptions={["sjukra"]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Dismiss the rest" }));
    expect(screen.getByText(/Dismiss 2 groups\?/)).toBeTruthy();
    expect(dismissEventCalls.length).toBe(0);

    fireEvent.click(screen.getByRole("button", { name: "Yes" }));
    await waitFor(() => expect(dismissEventCalls.length).toBe(3));
    const ids = dismissEventCalls.map((c) => c[0]).sort();
    expect(ids).toEqual([1, 2, 3]);
    // Bulk dismiss never carries a rule kind.
    expect(dismissEventCalls.every((c) => c[1] === null)).toBe(true);
  });

  it("Cancel backs out of the confirm without dismissing anything", () => {
    render(<UnsortedList day="2026-07-25" events={[slack1]} folderOptions={["sjukra"]} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss the rest" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Dismiss the rest" })).toBeTruthy();
    expect(dismissEventCalls.length).toBe(0);
  });
});

describe("filed events", () => {
  it("shows filed events collapsed under Auto-filed, not in the tray", () => {
    render(
      <UnsortedList
        day="2026-07-25"
        events={[slack1, ruleLabelled]}
        folderOptions={["sjukra", "AWS cert"]}
      />,
    );
    expect(screen.getByText(/Auto-filed · 1/)).toBeTruthy();
    // Collapsed by default — assert on `.open`, not DOM presence (the row
    // is still in the tree, just CSS-hidden by the browser).
    const disclosure = document.querySelector("details.auto-filed") as HTMLDetailsElement;
    expect(disclosure.open).toBe(false);
  });
});

describe("drops previous day's rows on navigation", () => {
  it("re-mounting with a new day and events shows only the new day's groups", () => {
    const dayBEvent: RoutedEvent = {
      id: 99,
      source: "slack",
      started_at: "2026-07-26T09:00:00Z",
      title: "day-b-channel",
      details: null,
      container: null,
      folder: null,
      label_origin: null,
      label_confidence: null,
    };
    const { rerender } = render(
      <UnsortedList key="2026-07-25" day="2026-07-25" events={[slack1]} folderOptions={[]} />,
    );
    expect(screen.getByText("sjukra")).toBeTruthy();

    rerender(
      <UnsortedList key="2026-07-26" day="2026-07-26" events={[dayBEvent]} folderOptions={[]} />,
    );

    expect(screen.queryByText("sjukra")).toBeNull();
    expect(screen.getByText("day-b-channel")).toBeTruthy();
  });
});
