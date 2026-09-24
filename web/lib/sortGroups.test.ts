import { describe, expect, it } from "bun:test";
import type { RoutedEvent } from "./types";
import { countLabel, groupEvents, ruleKindFor, ruleKindOnFirst } from "./sortGroups";

function ev(overrides: Partial<RoutedEvent>): RoutedEvent {
  return {
    id: 1,
    source: "slack",
    started_at: "2026-07-25T10:00:00Z",
    title: "sjukra",
    details: null,
    container: null,
    folder: null,
    label_origin: null,
    label_confidence: null,
    ...overrides,
  };
}

describe("groupEvents", () => {
  it("groups Slack events by title (channel/DM name)", () => {
    const events = [
      ev({ id: 1, source: "slack", title: "sjukra", started_at: "2026-07-25T09:00:00Z" }),
      ev({ id: 2, source: "slack", title: "sjukra", started_at: "2026-07-25T10:00:00Z" }),
      ev({ id: 3, source: "slack", title: "vitinn-team", started_at: "2026-07-25T11:00:00Z" }),
    ];
    const groups = groupEvents(events);
    expect(groups).toHaveLength(2);
    const sjukra = groups.find((g) => g.label === "sjukra")!;
    expect(sjukra.count).toBe(2);
    expect(sjukra.events.map((e) => e.id)).toEqual([1, 2]);
  });

  it("groups Firefox events by hostname", () => {
    const events = [
      ev({ id: 1, source: "firefox", title: "Docs", details: "https://docs.rs/tokio/latest" }),
      ev({ id: 2, source: "firefox", title: "Docs 2", details: "https://docs.rs/serde" }),
      ev({ id: 3, source: "firefox", title: "Other", details: "https://example.com/x" }),
    ];
    const groups = groupEvents(events);
    expect(groups.find((g) => g.label === "docs.rs")?.count).toBe(2);
    expect(groups.find((g) => g.label === "example.com")?.count).toBe(1);
  });

  it("groups github.com by org/repo, not the bare hostname", () => {
    const events = [
      ev({ id: 1, source: "firefox", title: "PR #9", details: "https://github.com/org/vitinn-infra/pull/9" }),
      ev({ id: 2, source: "firefox", title: "Issue #3", details: "https://github.com/org/vitinn-infra/issues/3" }),
      ev({ id: 3, source: "firefox", title: "Other repo", details: "https://github.com/other/repo" }),
    ];
    const groups = groupEvents(events);
    expect(groups.find((g) => g.label === "github.com/org/vitinn-infra")?.count).toBe(2);
    expect(groups.find((g) => g.label === "github.com/other/repo")?.count).toBe(1);
  });

  it("falls back to the event's own title when details isn't a parseable URL", () => {
    const events = [ev({ id: 1, source: "firefox", title: "Weird tab", details: "not a url" })];
    const groups = groupEvents(events);
    expect(groups[0].label).toBe("Weird tab");
  });

  it("tracks the earliest/latest times and the latest preview", () => {
    const events = [
      ev({ id: 1, title: "sjukra", started_at: "2026-07-25T09:00:00Z", details: "first" }),
      ev({ id: 2, title: "sjukra", started_at: "2026-07-25T14:10:00Z", details: "last" }),
      ev({ id: 3, title: "sjukra", started_at: "2026-07-25T11:00:00Z", details: "middle" }),
    ];
    const [g] = groupEvents(events);
    expect(g.earliestAt).toBe("2026-07-25T09:00:00Z");
    expect(g.latestAt).toBe("2026-07-25T14:10:00Z");
    expect(g.latestPreview).toBe("last");
  });

  it("sorts by count desc, then latest time desc", () => {
    const events = [
      ev({ id: 1, title: "small", started_at: "2026-07-25T18:00:00Z" }), // 1 event, latest overall
      ev({ id: 2, title: "big", started_at: "2026-07-25T09:00:00Z" }),
      ev({ id: 3, title: "big", started_at: "2026-07-25T10:00:00Z" }),
      ev({ id: 4, title: "also-big", started_at: "2026-07-25T09:30:00Z" }),
      ev({ id: 5, title: "also-big", started_at: "2026-07-25T09:45:00Z" }),
    ];
    const groups = groupEvents(events);
    // "big" and "also-big" both have 2 events (beat "small"'s 1); of the
    // two, "big"'s latest event (10:00) is after "also-big"'s (09:45).
    expect(groups.map((g) => g.label)).toEqual(["big", "also-big", "small"]);
  });
});

describe("countLabel", () => {
  it("says messages for Slack", () => {
    expect(countLabel({ source: "slack", count: 7 })).toBe("7 messages");
    expect(countLabel({ source: "slack", count: 1 })).toBe("1 message");
  });

  it("says visits for Firefox", () => {
    expect(countLabel({ source: "firefox", count: 3 })).toBe("3 visits");
    expect(countLabel({ source: "firefox", count: 1 })).toBe("1 visit");
  });
});

describe("ruleKindFor", () => {
  it("is slack_channel for slack, domain for everything else", () => {
    expect(ruleKindFor("slack")).toBe("slack_channel");
    expect(ruleKindFor("firefox")).toBe("domain");
  });
});

describe("ruleKindOnFirst", () => {
  it("puts the rule kind on the first entry only", () => {
    expect(ruleKindOnFirst(3, "domain")).toEqual(["domain", null, null]);
  });

  it("is all null when no rule kind is given", () => {
    expect(ruleKindOnFirst(3, null)).toEqual([null, null, null]);
  });

  it("handles a single-event group", () => {
    expect(ruleKindOnFirst(1, "slack_channel")).toEqual(["slack_channel"]);
  });
});
