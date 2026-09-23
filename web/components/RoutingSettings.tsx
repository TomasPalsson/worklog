"use client";

// Browser/Slack event routing controls for the Settings panel (spec 003
// FR-18/FR-19): the work-hours + threshold knobs behind routing decisions,
// last-seen source activity, and the hard-rule list with delete. Split out
// of SettingsPanel.tsx to keep that file under the size gate — these
// pieces don't participate in the main settings save/hydrate cycle except
// for the two controlled inputs, which the parent still owns.

import { useEffect, useState } from "react";
import { Loader2, Trash2 } from "lucide-react";
import type { Rule, RoutingStatus } from "@/lib/types";
import { deleteRule, fetchRoutingRules, fetchRoutingStatus } from "@/app/actions";
import { toast } from "@/lib/toast";

interface FieldsProps {
  workHours: string;
  onWorkHoursChange: (v: string) => void;
  routeThreshold: string;
  onRouteThresholdChange: (v: string) => void;
}

/** Work-hours window (FR-04) and model-guess threshold (FR-08). Plain
 * controlled inputs — the parent settings form owns save/hydrate. */
export function RoutingFields({
  workHours,
  onWorkHoursChange,
  routeThreshold,
  onRouteThresholdChange,
}: FieldsProps) {
  return (
    <div className="settings-grid-2">
      <label className="settings-field settings-field-narrow">
        <span>Work hours</span>
        <input
          type="text"
          value={workHours}
          placeholder="Mon-Fri 09:00-17:00"
          autoComplete="off"
          onChange={(e) => onWorkHoursChange(e.target.value)}
        />
      </label>
      <label className="settings-field settings-field-narrow">
        <span>Match threshold (0–1)</span>
        <input
          type="number"
          min={0}
          max={1}
          step={0.01}
          value={routeThreshold}
          autoComplete="off"
          onChange={(e) => onRouteThresholdChange(e.target.value)}
        />
      </label>
    </div>
  );
}

function formatStatusTime(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString() : "never";
}

function StatusList({ status }: { status: RoutingStatus | null }) {
  return (
    <ul className="settings-hint">
      <li>Last heartbeat: {formatStatusTime(status?.last_heartbeat ?? null)}</li>
      <li>Last Slack collect: {formatStatusTime(status?.last_slack ?? null)}</li>
      <li>Model helper: {status?.laya_reachable ? "reachable" : "unreachable"}</li>
    </ul>
  );
}

function RuleRow({
  rule,
  busy,
  onDelete,
}: {
  rule: Rule;
  busy: boolean;
  onDelete: (id: number) => void;
}) {
  return (
    <li>
      <span>
        {rule.kind}: {rule.pattern} → {rule.folder}
      </span>{" "}
      <button
        type="button"
        className="icon-btn"
        aria-label={`Delete rule for ${rule.pattern}`}
        disabled={busy}
        onClick={() => onDelete(rule.id)}
      >
        {busy ? <Loader2 className="spin" size={14} /> : <Trash2 size={14} />}
      </button>
    </li>
  );
}

/** Last heartbeat/Slack collect + model-helper reachability (FR-19), and
 * the hard-rule list with delete (FR-18). Self-loads on mount — read-only
 * except for delete, so it doesn't hook into the settings save cycle. */
export function RoutingStatusAndRules({ day }: { day: string }) {
  const [rules, setRules] = useState<Rule[]>([]);
  const [status, setStatus] = useState<RoutingStatus | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);

  useEffect(() => {
    void (async () => {
      const [rulesResult, statusResult] = await Promise.all([
        fetchRoutingRules(),
        fetchRoutingStatus(),
      ]);
      if (rulesResult.ok) setRules(rulesResult.data);
      if (statusResult.ok) setStatus(statusResult.data);
    })();
  }, []);

  async function onDelete(id: number) {
    setBusyId(id);
    const r = await deleteRule(id, day);
    setBusyId(null);
    if (!r.ok) {
      toast.error(`Delete rule failed — ${r.error}`);
      return;
    }
    setRules((rs) => rs.filter((x) => x.id !== id));
    toast.ok("Rule deleted.");
  }

  return (
    <>
      <h4>Source status</h4>
      <StatusList status={status} />

      <h4>Hard rules</h4>
      {rules.length === 0 ? (
        <p className="settings-hint">No rules yet.</p>
      ) : (
        <ul className="settings-hint">
          {rules.map((r) => (
            <RuleRow key={r.id} rule={r} busy={busyId === r.id} onDelete={onDelete} />
          ))}
        </ul>
      )}
    </>
  );
}
