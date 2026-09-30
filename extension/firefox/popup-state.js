/*
 * Pure popup view-state. Shapes mirrored from the daemon contract
 * (rust/crates/worklog-core/src/routing_contract.rs):
 *
 * #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
 * pub struct BrowserStatus {
 *     pub in_work_hours: bool,
 *     pub recording_until: Option<DateTime<Utc>>, // None when off or expired
 *     pub minutes_today: i64,
 *     pub work_hours: String,                     // e.g. "Mon-Fri 09:00-17:00"
 * }
 *
 * storage.local `lastHeartbeat` = { ts, title, url, stored, reason }; `reason` is
 * one of outside_work_hours, incognito, personal_container, paused, idle,
 * unfocused, daemon_down, or null when stored.
 *
 * GET  /browser/status                      -> 200 BrowserStatus
 * POST /browser/recording { "on": true }    -> sets recording_until; 200 BrowserStatus
 * POST /browser/recording { "on": false }   -> clears recording_until; 200 BrowserStatus
 */

const MINUTE_MILLISECONDS = 60_000;

export function formatMinutes(minutes) {
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  if (hours === 0) return `${remainder}m`;
  return remainder === 0 ? `${hours}h` : `${hours}h ${remainder}m`;
}

export function formatTimeLeft(recordingUntil, now) {
  if (recordingUntil === null) return null;
  const millisecondsLeft = new Date(recordingUntil) - now;
  if (millisecondsLeft <= 0) return null;
  const minutes = Math.floor(millisecondsLeft / MINUTE_MILLISECONDS);
  return minutes === 0 ? "<1m" : formatMinutes(minutes);
}

function classify({ status, daemonDown, paused, reason, overrideActive }) {
  if (daemonDown || status === null) return "daemon_down";
  if (reason === "incognito" || reason === "personal_container") return "not_counted";
  if (paused) return "paused";
  if (reason === "idle" || reason === "unfocused") return "not_counted";
  return status.in_work_hours || overrideActive ? "recording" : "outside";
}

const COPY = {
  daemon_down: { headline: "Worklog isn't running", detail: "Start the daemon to record browsing." },
  paused: { headline: "Paused", detail: "Nothing is recorded until you resume." },
  outside: { headline: "Outside work hours", detail: "Browsing isn't recorded right now." },
  recording: { headline: "Recording", detail: "This tab is counted." },
};

const NOT_COUNTED_DETAIL = {
  incognito: "Private windows are never recorded.",
  personal_container: "The Personal container is never recorded.",
  idle: "You're idle, so nothing is recorded.",
  unfocused: "The browser isn't focused, so nothing is recorded.",
};

function buttons({ status, paused, overrideActive }) {
  if (paused) return { primary: { label: "Resume", action: "resume" }, secondary: null };
  if (status.in_work_hours) {
    return {
      primary: { label: "Pause", action: "pause" },
      secondary: overrideActive ? { label: "Stop recording", action: "stop" } : null,
    };
  }
  if (overrideActive) return { primary: { label: "Stop recording", action: "stop" }, secondary: null };
  return { primary: { label: "Record now", action: "start" }, secondary: null };
}

export function viewState({ status, daemonDown, paused, lastHeartbeat, now }) {
  const reason = lastHeartbeat?.reason ?? null;
  const overrideActive =
    status !== null && status.recording_until !== null && new Date(status.recording_until) > now;
  const kind = classify({ status, daemonDown, paused, reason, overrideActive });
  const copy =
    kind === "not_counted"
      ? { headline: "Not counted", detail: NOT_COUNTED_DETAIL[reason] }
      : COPY[kind];
  const base = { status: kind, ...copy };

  if (kind === "daemon_down") {
    return { ...base, primary: null, secondary: null, tab: null, minutesToday: "—", timeLeft: null };
  }
  return {
    ...base,
    ...buttons({ status, paused, overrideActive }),
    tab: kind === "recording" && lastHeartbeat ? { title: lastHeartbeat.title, url: lastHeartbeat.url } : null,
    minutesToday: formatMinutes(status.minutes_today),
    timeLeft: formatTimeLeft(status.recording_until, now),
  };
}
