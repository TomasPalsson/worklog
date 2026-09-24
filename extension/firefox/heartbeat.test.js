import { describe, expect, test } from "bun:test";
import { buildHeartbeat, shouldSend } from "./heartbeat.js";

describe("shouldSend", () => {
  const base = {
    paused: false,
    incognito: false,
    containerName: null,
    idleState: "active",
    windowFocused: true,
  };

  test("true when active, focused, not paused/incognito/personal", () => {
    expect(shouldSend(base)).toBe(true);
  });

  test("false when paused", () => {
    expect(shouldSend({ ...base, paused: true })).toBe(false);
  });

  test("false when incognito", () => {
    expect(shouldSend({ ...base, incognito: true })).toBe(false);
  });

  test("false in the Personal container", () => {
    expect(shouldSend({ ...base, containerName: "Personal" })).toBe(false);
  });

  test("false when idle", () => {
    expect(shouldSend({ ...base, idleState: "idle" })).toBe(false);
  });

  test("false when locked", () => {
    expect(shouldSend({ ...base, idleState: "locked" })).toBe(false);
  });

  test("false with no focused window", () => {
    expect(shouldSend({ ...base, windowFocused: false })).toBe(false);
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
