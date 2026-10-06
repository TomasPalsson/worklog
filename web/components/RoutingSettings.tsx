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
  abstainMargin: string;
  onAbstainMarginChange: (v: string) => void;
  runnerUpRatio: string;
  onRunnerUpRatioChange: (v: string) => void;
}

/** Work-hours window (FR-04) and Verdict's two filing ratios (FR-05).
 * Plain controlled inputs — the parent settings form owns save/hydrate. */
export function RoutingFields({
  workHours,
  onWorkHoursChange,
  abstainMargin,
  onAbstainMarginChange,
  runnerUpRatio,
  onRunnerUpRatioChange,
}: FieldsProps) {
  return (
    <>
      <label className="settings-field settings-field-narrow">
        <span>Work hours</span>
        <input
          type="text"
          value={workHours}
          placeholder="Mon-Fri 09:00-17:00"
          autoComplete="off"
          onChange={(e) => onWorkHoursChange(e.target.value)}
        />
        <small>Browser tabs outside these hours are ignored.</small>
      </label>
      <details className="set-advanced">
        <summary>Advanced: how sure the model must be</summary>
        <p className="settings-hint">
          Higher numbers mean fewer guesses and more items left in Unsorted. Both go from
          1 to 5.
        </p>
        <div className="settings-grid-2">
          <label className="settings-field settings-field-narrow">
            <span>Lead over &ldquo;not sure&rdquo; (abstain margin)</span>
            <input
              type="number"
              min={1}
              max={5}
              step={0.01}
              value={abstainMargin}
              autoComplete="off"
              onChange={(e) => onAbstainMarginChange(e.target.value)}
            />
            <small>1.2 means the best project must be 20% more likely than &ldquo;not sure&rdquo;.</small>
          </label>
          <label className="settings-field settings-field-narrow">
            <span>Lead over second best (runner-up ratio)</span>
            <input
              type="number"
              min={1}
              max={5}
              step={0.01}
              value={runnerUpRatio}
              autoComplete="off"
              onChange={(e) => onRunnerUpRatioChange(e.target.value)}
            />
            <small>1.1 means it must be 10% more likely than the next project.</small>
          </label>
        </div>
      </details>
    </>
  );
}

function formatStatusTime(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString() : "never";
}

function StatusList({ status }: { status: RoutingStatus | null }) {
  const up = status?.classifier_reachable;
  return (
    <ul className="set-status">
      <li data-ok={!!status?.last_heartbeat}>Last heartbeat: {formatStatusTime(status?.last_heartbeat ?? null)}</li>
      <li data-ok={!!status?.last_slack}>Last Slack collect: {formatStatusTime(status?.last_slack ?? null)}</li>
      <li data-ok={!!up}>Model helper: {up ? "reachable" : "unreachable"}</li>
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
      <span className="set-rule-kind">{rule.kind}</span>
      <code>{rule.pattern}</code>
      <span aria-hidden="true">→</span>
      <span className="set-rule-folder">{rule.folder === "__ignore__" ? "Ignored" : rule.folder}</span>
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
  // A failed load must not read as "no rules" / "never" — say it failed.
  const [rulesFailed, setRulesFailed] = useState(false);
  const [statusFailed, setStatusFailed] = useState(false);

  useEffect(() => {
    void (async () => {
      const [rulesResult, statusResult] = await Promise.all([
        fetchRoutingRules(),
        fetchRoutingStatus(),
      ]);
      if (rulesResult.ok) setRules(rulesResult.data);
      else setRulesFailed(true);
      if (statusResult.ok) setStatus(statusResult.data);
      else setStatusFailed(true);
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
      <h3>Is it working?</h3>
      {statusFailed ? (
        <p className="settings-hint verdict-unknown">
          Couldn&rsquo;t check right now. Reload the page to try again.
        </p>
      ) : (
        <StatusList status={status} />
      )}

      <h3>Your rules</h3>
      <RuleList rules={rulesFailed ? null : rules} busyId={busyId} onDelete={onDelete} />
    </>
  );
}

/** `rules === null` means the load failed — never shown as "no rules". */
function RuleList({
  rules,
  busyId,
  onDelete,
}: {
  rules: Rule[] | null;
  busyId: number | null;
  onDelete: (id: number) => void;
}) {
  if (rules === null) {
    return (
      <p className="settings-hint verdict-unknown">
        Couldn&rsquo;t load your rules. They are still saved; reload the page to try again.
      </p>
    );
  }
  if (rules.length === 0) {
    return (
      <p className="settings-hint">
        No rules yet. Pick a project for an item in a day&rsquo;s Unsorted list and tick
        &ldquo;always&rdquo; to add one.
      </p>
    );
  }
  return (
    <ul className="set-rules">
      {rules.map((r) => (
        <RuleRow key={r.id} rule={r} busy={busyId === r.id} onDelete={onDelete} />
      ))}
    </ul>
  );
}
