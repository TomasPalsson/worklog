import { describe, expect, test } from "bun:test";
import { formatEventTime, previewDetails, sourceLabel } from "./format-event";

describe("formatEventTime", () => {
  test("returns HH:MM for a valid RFC3339 timestamp", () => {
    // Timezone-sensitive — we assert the length + shape rather than an
    // exact string so CI doesn't drift between UTC and local-clock boxes.
    const s = formatEventTime("2026-04-18T09:30:15Z");
    expect(s).toMatch(/^\d{2}:\d{2}$/);
  });

  test("returns the raw input on unparseable timestamps", () => {
    expect(formatEventTime("not a date")).toBe("not a date");
  });
});

describe("previewDetails", () => {
  test("returns { preview: '', truncated: false } for null details", () => {
    const r = previewDetails(null);
    expect(r.preview).toBe("");
    expect(r.truncated).toBe(false);
  });

  test("short details pass through unchanged", () => {
    const r = previewDetails("hello world", 160);
    expect(r.preview).toBe("hello world");
    expect(r.truncated).toBe(false);
  });

  test("long details are sliced at maxChars with an ellipsis marker", () => {
    const long = "x".repeat(300);
    const r = previewDetails(long, 100);
    expect(r.truncated).toBe(true);
    expect(r.preview.endsWith("…")).toBe(true);
    // 100 chars + the ellipsis
    expect([...r.preview].length).toBe(101);
  });

  test("slicing is char-safe for multi-byte UTF-8", () => {
    // "日" is 3 bytes but 1 char. If we were byte-slicing at 50 we'd
    // corrupt the payload; char-slicing at 50 keeps it clean.
    const long = "日".repeat(200);
    const r = previewDetails(long, 50);
    expect(r.truncated).toBe(true);
    // 50 chars preserved + the ellipsis
    expect([...r.preview].filter((c) => c === "日").length).toBe(50);
  });
});

describe("sourceLabel", () => {
  test("returns a human-readable label per source kind", () => {
    expect(sourceLabel("github")).toBe("GitHub");
    expect(sourceLabel("claude")).toBe("Claude");
    expect(sourceLabel("gcal")).toBe("Calendar");
    expect(sourceLabel("jira")).toBe("Jira");
    expect(sourceLabel("other")).toBe("other");
  });
});
