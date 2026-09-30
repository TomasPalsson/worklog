import { describe, expect, test } from "bun:test";
import { formatMinutes, formatTimeLeft, viewState } from "./popup-state.js";

const now = new Date("2026-09-30T12:00:00Z");
const future = "2026-09-30T15:30:00Z";
const past = "2026-09-30T11:00:00Z";
const status = (overrides = {}) => ({
  in_work_hours: true,
  recording_until: null,
  minutes_today: 75,
  work_hours: "Mon-Fri 09:00-17:00",
  ...overrides,
});
const tabBeat = (reason = null) => ({
  ts: "2026-09-30T11:59:00Z",
  title: "Docs",
  url: "https://example.com/docs",
  stored: reason === null,
  reason,
});
const input = (overrides = {}) => ({
  status: status(),
  daemonDown: false,
  paused: false,
  lastHeartbeat: tabBeat(),
  now,
  ...overrides,
});

describe("formatMinutes", () => {
  test.each([
    [0, "0 min"],
    [45, "45 min"],
    [60, "1 h 00 min"],
    [75, "1 h 15 min"],
    [125, "2 h 05 min"],
  ])("%d -> %s", (minutes, text) => {
    expect(formatMinutes(minutes)).toBe(text);
  });
});

describe("formatTimeLeft", () => {
  test("hours and minutes", () => {
    expect(formatTimeLeft(future, now)).toBe("3 h 30 min");
  });
  test("minutes only", () => {
    expect(formatTimeLeft("2026-09-30T12:20:00Z", now)).toBe("20 min");
  });
  test("under a minute", () => {
    expect(formatTimeLeft("2026-09-30T12:00:30Z", now)).toBe("<1 min");
  });
  test("null when none or expired", () => {
    expect(formatTimeLeft(null, now)).toBeNull();
    expect(formatTimeLeft(past, now)).toBeNull();
    expect(formatTimeLeft("2026-09-30T12:00:00Z", now)).toBeNull();
  });
});

describe("viewState", () => {
  test("daemon down: no primary, wins over everything", () => {
    const state = viewState(input({ daemonDown: true, paused: true, lastHeartbeat: tabBeat("incognito") }));
    expect(state.status).toBe("daemon_down");
    expect(state.primary).toBeNull();
    expect(state.secondary).toBeNull();
    expect(state.tab).toBeNull();
  });

  test("null status is treated as daemon down", () => {
    expect(viewState(input({ status: null })).status).toBe("daemon_down");
  });

  test("recording in work hours: pause, tab and minutes shown", () => {
    const state = viewState(input());
    expect(state.status).toBe("recording");
    expect(state.primary.action).toBe("pause");
    expect(state.secondary).toBeNull();
    expect(state.tab).toEqual({ title: "Docs", url: "https://example.com/docs" });
    expect(state.minutesToday).toBe("1 h 15 min");
    expect(state.timeLeft).toBeNull();
  });

  test.each(["incognito", "personal_container"])("%s beats paused", (reason) => {
    const state = viewState(input({ paused: true, lastHeartbeat: tabBeat(reason) }));
    expect(state.status).toBe("not_counted");
    expect(state.tab).toBeNull();
  });

  test("paused: resume, beats idle", () => {
    const state = viewState(input({ paused: true, lastHeartbeat: tabBeat("idle") }));
    expect(state.status).toBe("paused");
    expect(state.primary.action).toBe("resume");
    expect(state.secondary).toBeNull();
  });

  test.each(["idle", "unfocused"])("%s is not counted", (reason) => {
    const state = viewState(input({ lastHeartbeat: tabBeat(reason) }));
    expect(state.status).toBe("not_counted");
    expect(state.primary.action).toBe("pause");
  });

  test("outside work hours without override: start", () => {
    const state = viewState(
      input({ status: status({ in_work_hours: false }), lastHeartbeat: tabBeat("outside_work_hours") }),
    );
    expect(state.status).toBe("outside");
    expect(state.primary.action).toBe("start");
    expect(state.secondary).toBeNull();
  });

  test("expired override outside work hours: start", () => {
    const state = viewState(input({ status: status({ in_work_hours: false, recording_until: past }) }));
    expect(state.status).toBe("outside");
    expect(state.primary.action).toBe("start");
  });

  test("override outside work hours: recording with stop and time left", () => {
    const state = viewState(input({ status: status({ in_work_hours: false, recording_until: future }) }));
    expect(state.status).toBe("recording");
    expect(state.primary.action).toBe("stop");
    expect(state.secondary).toBeNull();
    expect(state.timeLeft).toBe("3 h 30 min");
  });

  test("override inside work hours: pause primary, stop secondary", () => {
    const state = viewState(input({ status: status({ recording_until: future }) }));
    expect(state.primary.action).toBe("pause");
    expect(state.secondary.action).toBe("stop");
  });

  test("FR-07 primary label for every rule", () => {
    const label = (overrides) => viewState(input(overrides)).primary?.label ?? null;
    const outside = status({ in_work_hours: false });
    expect(label({ daemonDown: true })).toBeNull();
    expect(label({ paused: true })).toBe("Resume");
    expect(label({ paused: true, status: outside })).toBe("Resume");
    expect(label({})).toBe("Pause");
    expect(label({ status: status({ in_work_hours: false, recording_until: future }) })).toBe("Stop recording");
    expect(label({ status: outside })).toBe("Start recording");
  });

  test("FR-08 paused outside work hours: paused", () => {
    const state = viewState(
      input({ paused: true, status: status({ in_work_hours: false }), lastHeartbeat: tabBeat("outside_work_hours") }),
    );
    expect(state.status).toBe("paused");
  });

  test("FR-08 incognito in work hours, not paused: not counted", () => {
    const state = viewState(input({ lastHeartbeat: tabBeat("incognito") }));
    expect(state.status).toBe("not_counted");
  });

  test("paused with override: no secondary", () => {
    const state = viewState(input({ paused: true, status: status({ recording_until: future }) }));
    expect(state.secondary).toBeNull();
  });
});
