import { describe, expect, test } from "bun:test";
import { formatMinutes, formatTimeLeft, viewState } from "./popup-state.js";

const now = new Date("2026-09-30T12:00:00Z");
const future = "2026-09-30T15:30:00Z";
const past = "2026-09-30T11:00:00Z";
const status = (over = {}) => ({
  in_work_hours: true,
  recording_until: null,
  minutes_today: 75,
  work_hours: "Mon-Fri 09:00-17:00",
  ...over,
});
const tabBeat = (reason = null) => ({
  ts: "2026-09-30T11:59:00Z",
  title: "Docs",
  url: "https://example.com/docs",
  stored: reason === null,
  reason,
});
const input = (over = {}) => ({
  status: status(),
  daemonDown: false,
  paused: false,
  lastHeartbeat: tabBeat(),
  now,
  ...over,
});

describe("formatMinutes", () => {
  test.each([
    [0, "0m"],
    [45, "45m"],
    [60, "1h"],
    [75, "1h 15m"],
    [125, "2h 5m"],
  ])("%d -> %s", (minutes, text) => {
    expect(formatMinutes(minutes)).toBe(text);
  });
});

describe("formatTimeLeft", () => {
  test("hours and minutes", () => {
    expect(formatTimeLeft(future, now)).toBe("3h 30m");
  });
  test("minutes only", () => {
    expect(formatTimeLeft("2026-09-30T12:20:00Z", now)).toBe("20m");
  });
  test("under a minute", () => {
    expect(formatTimeLeft("2026-09-30T12:00:30Z", now)).toBe("<1m");
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
    expect(state.minutesToday).toBe("1h 15m");
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
    expect(state.timeLeft).toBe("3h 30m");
  });

  test("override inside work hours: pause primary, stop secondary", () => {
    const state = viewState(input({ status: status({ recording_until: future }) }));
    expect(state.primary.action).toBe("pause");
    expect(state.secondary.action).toBe("stop");
  });

  test("paused with override: no secondary", () => {
    const state = viewState(input({ paused: true, status: status({ recording_until: future }) }));
    expect(state.secondary).toBeNull();
  });
});
