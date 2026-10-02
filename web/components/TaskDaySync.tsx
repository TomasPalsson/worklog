"use client";

import { useEffect, useRef, useState } from "react";
import { UploadCloud } from "lucide-react";

import { formatDuration } from "@/lib/format";
import { DaySent, type Common } from "./TaskDayTools";
import { SyncPreview } from "./TaskSyncPreview";

type Step =
  | { s: "idle" }
  | { s: "running" }
  | { s: "preview" }
  | { s: "sent"; msg: string }
  | { s: "nothing"; msg: string }
  | { s: "error"; msg: string };

type SyncData = { results?: { status: string; reason: string | null }[] };

/** Plain words for a sync that sent nothing: the first reason the daemon gave, else the likely causes. */
function nothingSent(taskKey: string, label: string, data: SyncData): string {
  const reason = data.results?.find((r) => r.reason)?.reason?.trim().replace(/\.$/, "");
  const head = `Nothing was sent to Tempo for ${taskKey} on ${label}`;
  return reason ? `${head}: ${reason}.` : `${head}. It may already be in Tempo, or have no hours.`;
}

type SyncProps = Common & { inTempo: boolean; changed: boolean };

/** Two-step Tempo sync for one ticket-day: dry run, preview, then send on confirm. */
export function useSync({ taskKey, actions, onSaved, onAnnounce, label, day, changed }: SyncProps) {
  const [step, setStep] = useState<Step>({ s: "idle" });
  const [sending, setSending] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (step.s === "idle" && refocus.current) trigger.current?.focus();
    refocus.current = false;
  }, [step.s]);
  const cancel = () => {
    refocus.current = true;
    setStep({ s: "idle" });
  };

  /** Shared tail of both runs: an error or an empty result becomes the step; null means it went through. */
  function failed(res: Awaited<ReturnType<typeof actions.runSync>>): Step | null {
    if (!res.ok) return { s: "error", msg: res.error };
    if (res.data.errors.length > 0) return { s: "error", msg: res.data.errors.join("; ") };
    if (res.data.synced === 0) return { s: "nothing", msg: nothingSent(taskKey, label, res.data) };
    return null;
  }

  async function dryRun() {
    setStep({ s: "running" });
    setStep(failed(await actions.runSync(day.day, true, taskKey)) ?? { s: "preview" });
  }

  async function send() {
    setSending(true);
    const res = await actions.runSync(day.day, false, taskKey);
    setSending(false);
    const bad = failed(res);
    if (bad) return setStep(bad);
    const hours = formatDuration(day.line_seconds);
    setStep({ s: "sent", msg: `${changed ? "Tempo updated" : "Sent to Tempo"} · ${hours}` });
    onAnnounce?.(`${changed ? "Updated" : "Sent"} ${hours} ${changed ? "in" : "to"} Tempo for ${taskKey} on ${label}.`);
    onSaved();
  }

  return { step, sending, trigger, dryRun, send, cancel };
}

export type Sync = ReturnType<typeof useSync>;

/** Small secondary button on the day row; hidden once the day is in Tempo or while the preview is open. */
export function SyncTrigger({ sync, label, inTempo, changed }: { sync: Sync; label: string; inTempo: boolean; changed: boolean }) {
  const { step } = sync;
  if (inTempo || step.s === "sent" || step.s === "preview") return null;
  return (
    <button
      ref={sync.trigger}
      type="button"
      className="task-btn-secondary task-day-trigger"
      aria-label={`${changed ? "Update Tempo" : "Send to Tempo"}, ${label}`}
      disabled={step.s === "running"}
      onClick={sync.dryRun}
    >
      <UploadCloud size={12} aria-hidden="true" />
      {step.s === "running" ? "Checking…" : changed ? "Update Tempo" : "Send to Tempo"}
    </button>
  );
}

/** The preview, outcome strip or error that follows a trigger click. */
export function SyncBody({ sync, label, day, changed }: Pick<SyncProps, "label" | "day" | "changed"> & { sync: Sync }) {
  const { step } = sync;
  return (
    <>
      {step.s === "preview" && (
        <SyncPreview label={label} day={day} changed={changed} sending={sync.sending} onSend={sync.send} onCancel={sync.cancel} />
      )}
      {step.s === "sent" && <DaySent focus>{step.msg}</DaySent>}
      {step.s === "nothing" && (
        <DaySent focus plain>
          {step.msg}
        </DaySent>
      )}
      {step.s === "error" && (
        <p role="alert" className="task-error">
          {step.msg}
        </p>
      )}
    </>
  );
}
