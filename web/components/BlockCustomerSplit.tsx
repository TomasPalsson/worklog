"use client";

// Block customer split line + inline editor (spec 005, B17/FR-10/FR-14). An
// empty `customer-slices` means the folder isn't multi-tenant → render
// nothing. Fetched lazily on mount so the day's card list never waits.

import { useEffect, useState, useTransition } from "react";
// Namespace import, not `import { fetchBillingRegistry }`: other test files
// (outside this unit) mock `@/app/actions` with their own partial export
// set, and a named import's static binding check would fail to link
// against that at module-load time. A namespace object degrades to
// `undefined` on a missing key instead, same as any other unmocked call.
import * as billingActions from "@/app/actions";
import {
  clearCustomerShares,
  fetchCustomerSlices,
  saveCustomerShares,
} from "@/app/tenant-actions";
import type { CustomerSlice, SplitOrigin } from "@/lib/tenants";

const ORIGIN_LABELS: Record<SplitOrigin, string> = {
  clues: "auto",
  manual: "set by you",
  fallback: "guess",
};

interface Part {
  customer: string | null;
  percent: number;
}

function displayName(customer: string | null): string {
  return customer ?? "Unresolved";
}

/** One part per slice, percent of the block's duration, highest share first. */
function sliceParts(slices: CustomerSlice[]): Part[] {
  const durations = slices.map((s) =>
    s.intervals.reduce((sum, [from, to]) => sum + (to - from), 0),
  );
  const total = durations.reduce((sum, d) => sum + d, 0);
  return slices
    .map((s, i) => ({
      customer: s.customer,
      percent: total > 0 ? Math.round((durations[i] / total) * 100) : 0,
    }))
    .sort((a, b) => b.percent - a.percent);
}

function totalSeconds(slices: CustomerSlice[]): number {
  return slices.reduce(
    (sum, s) => sum + s.intervals.reduce((a, [from, to]) => a + (to - from), 0),
    0,
  );
}

/** "1h 23m" for the editor's live per-row estimate. */
function formatApprox(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

function splitLineText(parts: Part[]): string {
  return parts.length === 1
    ? displayName(parts[0].customer)
    : parts.map((p) => `${displayName(p.customer)} ${p.percent}%`).join(" · ");
}

/** `customer: ""` = a Fallback slice with nothing resolved — the row
 * renders a <select> the Owner must fill in; Save stays disabled until
 * every row has one and the total is 100%, so nothing saves "Unresolved". */
interface Row {
  customer: string;
  percent: number;
}

function splitEvenly(rows: Row[]): Row[] {
  const n = rows.length;
  if (n === 0) return rows;
  const base = Math.floor(100 / n);
  const remainder = 100 - base * n;
  return rows.map((r, i) => ({ ...r, percent: base + (i < remainder ? 1 : 0) }));
}

/** The quiet, non-editing line: split % · origin tag · optional bar · Change. */
function SplitDisplay({
  parts,
  origin,
  onChange,
}: {
  parts: Part[];
  origin: SplitOrigin;
  onChange: () => void;
}) {
  return (
    <div className="block-clue-line customer-split">
      <span>{splitLineText(parts)}</span>
      <span className="customer-split-origin">{ORIGIN_LABELS[origin]}</span>
      {parts.length > 1 && (
        <div className="customer-split-bar" aria-hidden="true">
          {parts.map((p, i) => (
            <div
              key={`${p.customer ?? "unresolved"}-${i}`}
              style={{
                width: `${p.percent}%`,
                background: i % 2 === 0 ? "var(--sage)" : "var(--amber)",
              }}
            />
          ))}
        </div>
      )}
      <button type="button" className="customer-split-change" onClick={onChange}>Change</button>
    </div>
  );
}

function SplitEditorRow({
  row,
  customerOptions,
  approxSeconds,
  onSetCustomer,
  onPercent,
  onRemove,
}: {
  row: Row;
  customerOptions: string[];
  approxSeconds: number;
  onSetCustomer: (customer: string) => void;
  onPercent: (percent: number) => void;
  onRemove: () => void;
}) {
  const label = row.customer || "unresolved customer";
  return (
    <div className="reg-row split-row">
      {row.customer ? (
        <span className="split-row-name">{row.customer}</span>
      ) : (
        <select
          className="reg-input split-row-name-picker"
          aria-label="Choose a customer"
          value=""
          onChange={(e) => onSetCustomer(e.target.value)}
        >
          <option value="" disabled>Choose a customer…</option>
          {customerOptions.map((name) => (
            <option key={name} value={name}>{name}</option>
          ))}
        </select>
      )}
      <span className="split-row-pct">
        <input
          className="reg-input"
          type="number"
          step={5}
          min={0}
          max={100}
          value={row.percent}
          aria-label={`${label} share percent`}
          onChange={(e) => onPercent(Number(e.target.value))}
        />
        <span aria-hidden="true">%</span>
      </span>
      <span className="split-row-approx">≈ {formatApprox((approxSeconds * row.percent) / 100)}</span>
      <button type="button" className="icon-btn" aria-label={`Remove ${label}`} onClick={onRemove}>×</button>
    </div>
  );
}

interface EditorProps {
  rows: Row[];
  customerOptions: string[];
  totalDurationSeconds: number;
  origin: SplitOrigin;
  error: string | null;
  isPending: boolean;
  onUpdateRow: (index: number, percent: number) => void;
  onSetRowCustomer: (index: number, customer: string) => void;
  onRemoveRow: (index: number) => void;
  onAddRow: (customer: string) => void;
  onSplitEvenly: () => void;
  onReset: () => void;
  onCancel: () => void;
  onSave: () => void;
}

/** One row per customer, an "Add customer" select sourced from the full
 * registry (every customer, not just already-linked tenants), and a live
 * total that gates Save. */
function SplitEditor({
  rows,
  customerOptions,
  totalDurationSeconds,
  origin,
  error,
  isPending,
  onUpdateRow,
  onSetRowCustomer,
  onRemoveRow,
  onAddRow,
  onSplitEvenly,
  onReset,
  onCancel,
  onSave,
}: EditorProps) {
  const total = rows.reduce((sum, r) => sum + r.percent, 0);
  const canSave = rows.length > 0 && rows.every((r) => r.customer !== "") && total === 100;
  const addableCustomers = customerOptions.filter((name) => !rows.some((r) => r.customer === name));

  return (
    <div
      className="block-clue-line customer-split-editor"
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
    >
      {rows.map((row, i) => (
        <SplitEditorRow
          key={i}
          row={row}
          customerOptions={addableCustomers}
          approxSeconds={totalDurationSeconds}
          onSetCustomer={(customer) => onSetRowCustomer(i, customer)}
          onPercent={(percent) => onUpdateRow(i, percent)}
          onRemove={() => onRemoveRow(i)}
        />
      ))}

      <select
        className="reg-input split-add-select"
        aria-label="Add customer"
        value=""
        onChange={(e) => onAddRow(e.target.value)}
      >
        <option value="">Add customer…</option>
        {addableCustomers.map((name) => (
          <option key={name} value={name}>{name}</option>
        ))}
      </select>

      {total !== 100 && <p className="export-error" role="alert">Shares must add up to 100%</p>}
      {error && <p className="export-error" role="alert">{error}</p>}

      <div className="split-editor-footer">
        <p className={total === 100 ? "settings-hint" : "export-error"}>Total: {total}%</p>
        <div className="reg-actions">
          <button type="button" className="action-btn" onClick={onSplitEvenly}>Split evenly</button>
          {origin === "manual" && (
            <button type="button" className="action-btn" disabled={isPending} onClick={onReset}>Reset to auto</button>
          )}
          <button type="button" className="action-btn" onClick={onCancel}>Cancel</button>
          <button type="button" className="action-btn primary" disabled={!canSave || isPending} onClick={onSave}>Save</button>
        </div>
      </div>
    </div>
  );
}

/** Loads the block's slices once on mount. */
function useLoadedSlices(blockId: number) {
  const [slices, setSlices] = useState<CustomerSlice[] | null>(null);
  useEffect(() => {
    let alive = true;
    fetchCustomerSlices(blockId).then((r) => {
      if (alive) setSlices(r.ok ? r.data : []);
    });
    return () => {
      alive = false;
    };
  }, [blockId]);
  return [slices, setSlices] as const;
}

/** Every registry customer, deferred to editor-open so the card list
 * itself never pays for it. */
function useCustomerOptions(editing: boolean): string[] {
  const [options, setOptions] = useState<string[]>([]);
  useEffect(() => {
    if (!editing) return;
    let alive = true;
    billingActions.fetchBillingRegistry().then((r) => {
      if (!alive || !r.ok) return;
      setOptions(r.data.customers.map((c) => c.name).filter((n) => n.trim() !== ""));
    });
    return () => {
      alive = false;
    };
  }, [editing]);
  return options;
}

/** State + mutations behind the card, split out to stay under the size gate. */
function useCustomerSplit(blockId: number) {
  const [slices, setSlices] = useLoadedSlices(blockId);
  const [editing, setEditing] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);
  const customerOptions = useCustomerOptions(editing);
  const [error, setError] = useState<string | null>(null);
  const [isPending, start] = useTransition();

  async function refreshSlices() {
    const fresh = await fetchCustomerSlices(blockId);
    if (fresh.ok) setSlices(fresh.data);
  }

  function openEditor(parts: Part[]) {
    setRows(parts.map((p) => ({ customer: p.customer ?? "", percent: p.percent })));
    setError(null);
    setEditing(true);
  }

  function closeEditor() {
    setEditing(false);
    setError(null);
  }

  function save() {
    const shares: Record<string, number> = {};
    for (const r of rows) shares[r.customer] = r.percent / 100;
    start(async () => {
      const res = await saveCustomerShares(blockId, shares);
      if (!res.ok) return setError(res.error);
      await refreshSlices();
      setEditing(false);
    });
  }

  function reset() {
    start(async () => {
      const res = await clearCustomerShares(blockId);
      if (!res.ok) return setError(res.error);
      await refreshSlices();
      setEditing(false);
    });
  }

  return {
    slices,
    editing,
    rows,
    setRows,
    customerOptions,
    error,
    isPending,
    openEditor,
    closeEditor,
    save,
    reset,
  };
}

export function BlockCustomerSplit({ blockId }: { blockId: number }) {
  const s = useCustomerSplit(blockId);

  if (s.slices === null || s.slices.length === 0) return null;

  const parts = sliceParts(s.slices);
  const origin = s.slices[0].origin;

  if (!s.editing) {
    return <SplitDisplay parts={parts} origin={origin} onChange={() => s.openEditor(parts)} />;
  }

  return (
    <SplitEditor
      rows={s.rows}
      customerOptions={s.customerOptions}
      totalDurationSeconds={totalSeconds(s.slices)}
      origin={origin}
      error={s.error}
      isPending={s.isPending}
      onUpdateRow={(index, percent) =>
        s.setRows((rs) => rs.map((r, i) => (i === index ? { ...r, percent } : r)))
      }
      onSetRowCustomer={(index, customer) =>
        s.setRows((rs) => rs.map((r, i) => (i === index ? { ...r, customer } : r)))
      }
      onRemoveRow={(index) => s.setRows((rs) => rs.filter((_, i) => i !== index))}
      onAddRow={(customer) => {
        if (!customer || s.rows.some((r) => r.customer === customer)) return;
        s.setRows((rs) => [...rs, { customer, percent: 0 }]);
      }}
      onSplitEvenly={() => s.setRows((rs) => splitEvenly(rs))}
      onReset={s.reset}
      onCancel={s.closeEditor}
      onSave={s.save}
    />
  );
}
