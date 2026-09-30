import { describe, expect, test } from "bun:test";
import { buildHeartbeat, skipReason, storableTab } from "./heartbeat.js";

describe("skipReason", () => {
  const base = {
    paused: false,
    incognito: false,
    containerName: null,
    idleState: "active",
    windowFocused: true,
  };

  test("null when nothing suppresses the heartbeat", () => {
    expect(skipReason(base)).toBeNull();
  });

  test("names each suppressor", () => {
    expect(skipReason({ ...base, paused: true })).toBe("paused");
    expect(skipReason({ ...base, incognito: true })).toBe("incognito");
    expect(skipReason({ ...base, containerName: "Personal" })).toBe("personal_container");
    expect(skipReason({ ...base, idleState: "idle" })).toBe("idle");
    expect(skipReason({ ...base, idleState: "locked" })).toBe("idle");
    expect(skipReason({ ...base, windowFocused: false })).toBe("unfocused");
  });

  test("reports the first suppressor in spec order", () => {
    const everything = { paused: true, incognito: true, containerName: "Personal", idleState: "idle", windowFocused: false };
    expect(skipReason(everything)).toBe("incognito");
    expect(skipReason({ ...everything, incognito: false })).toBe("personal_container");
    expect(skipReason({ ...everything, incognito: false, containerName: null })).toBe("paused");
    expect(skipReason({ ...everything, incognito: false, containerName: null, paused: false })).toBe("idle");
  });

  test("privacy reasons outrank paused", () => {
    expect(skipReason({ ...base, paused: true, containerName: "Personal" })).toBe("personal_container");
    expect(skipReason({ ...base, paused: true, incognito: true })).toBe("incognito");
  });
});

describe("storableTab", () => {
  const tab = { url: "https://example.com/", title: "Example", incognito: false };

  test("keeps title and url of a normal tab", () => {
    expect(storableTab(tab, null)).toEqual({ title: "Example", url: "https://example.com/" });
  });

  test("hides an incognito tab", () => {
    expect(storableTab({ ...tab, incognito: true }, null)).toEqual({ title: null, url: null });
  });

  test("hides a tab in the Personal container", () => {
    expect(storableTab(tab, "Personal")).toEqual({ title: null, url: null });
  });

  test("returns nulls when there is no tab", () => {
    expect(storableTab(null, null)).toEqual({ title: null, url: null });
  });
});

describe("buildHeartbeat", () => {
  test("carries url, title, container and incognito with a UTC ISO timestamp", () => {
    const tab = { url: "https://aws.tomasari.is/", title: "AWS cert", incognito: false };
    const now = new Date("2026-09-23T10:00:00.000Z");
    expect(buildHeartbeat(tab, "Work", now)).toEqual({
      ts: "2026-09-23T10:00:00.000Z",
      url: "https://aws.tomasari.is/",
      title: "AWS cert",
      container: "Work",
      incognito: false,
    });
  });

  test("carries null container for the default container", () => {
    const tab = { url: "https://example.com/", title: "Example", incognito: true };
    const now = new Date("2026-09-23T10:00:00.000Z");
    const heartbeat = buildHeartbeat(tab, null, now);
    expect(heartbeat.container).toBeNull();
    expect(heartbeat.incognito).toBe(true);
  });
});
