"use client";

// Google Calendar card: shows whether the collector's token file exists
// and runs the daemon's browser login on one click. Polls its own copy of
// the status so the surrounding settings form (and its unsaved edits)
// is never re-hydrated.

import { useEffect, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";
import { connectGcal, fetchSettings } from "@/app/actions";
import type { GcalStatus } from "@/lib/types";
import { GoogleCalendarMark } from "@/components/SourceIcon";

const POLL_MS = 3000;
const POLL_LIMIT_MS = 5 * 60 * 1000;

function useGcalConnect(initial: boolean) {
  const [connected, setConnected] = useState(initial);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [waiting, setWaiting] = useState(false);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  const stop = () => {
    if (timer.current) clearInterval(timer.current);
    timer.current = null;
    setWaiting(false);
  };
  useEffect(() => stop, []);
  useEffect(() => setConnected(initial), [initial]);

  const poll = () => {
    const started = Date.now();
    setWaiting(true);
    timer.current = setInterval(async () => {
      const r = await fetchSettings();
      if (r.ok && r.data.gcal?.connected) {
        setConnected(true);
        setNote(null);
        stop();
      } else if (Date.now() - started > POLL_LIMIT_MS) {
        setNote(null);
        setError("Timed out waiting for Google sign-in. Try again.");
        stop();
      }
    }, POLL_MS);
  };

  const connect = async () => {
    setError(null);
    const r = await connectGcal();
    if (!r.ok) return setError(r.error);
    setNote("Opened Google sign-in in your browser…");
    if (!timer.current) poll();
  };
  return { connected, note, error, waiting, connect };
}

export function GcalConnect({ gcal }: { gcal: GcalStatus }) {
  const { connected, note, error, waiting, connect } = useGcalConnect(gcal.connected);
  return (
    <details className="set-service" open={!connected}>
      <summary>
        <span className="set-service-icon" aria-hidden="true">
          <GoogleCalendarMark size={18} />
        </span>
        <span className="set-service-name">Google Calendar</span>
        <span className="set-pill" data-status={connected ? "on" : "off"}>
          {connected ? "Connected" : "Not set up"}
        </span>
        <span className="set-service-toggle" aria-hidden="true">
          <span className="set-when-closed">Edit</span>
          <span className="set-when-open">Hide</span>
          <ChevronDown size={16} />
        </span>
      </summary>
      <div className="settings-grid-2">
        <button type="button" className="action-btn" disabled={waiting} onClick={() => void connect()}>
          {connected ? "Reconnect" : "Connect Google Calendar"}
        </button>
        {note && <p className="settings-hint">{note}</p>}
        {error && (
          <p className="settings-hint" role="alert">
            {error}
          </p>
        )}
      </div>
    </details>
  );
}
