"use client";

// Block customer split line + inline editor (spec 005, B17/FR-10/FR-14): a
// multi-tenant folder like vitinn-infra bills by clue-computed shares
// unless the Owner hand-sets them. Empty `customer-slices` means the
// block's folder isn't multi-tenant, so the whole thing renders nothing —
// no noise on the vast majority of blocks. Fetched lazily on mount so the
// day's card list never waits on it.

import { useEffect, useState, useTransition } from "react";
import {
  clearCustomerShares,
  fetchCustomerSlices,
  fetchTenants,
  saveCustomerShares,
} from "@/app/tenant-actions";
import type { CustomerSlice, SplitOrigin } from "@/lib/tenants";

const ORIGIN_LABELS: Record<SplitOrigin, string> = {
  clues: "auto",
  manual: "set by you",
  fallback: "guess",
};

interface Part {
  customer: string;
  percent: number;
}

/** One part per slice, percent of the block's total duration — rounded for
 * display, exact fractions are only computed again on save. */
function sliceParts(slices: CustomerSlice[]): Part[] {
  const durations = slices.map((s) =>
    s.intervals.reduce((sum, [from, to]) => sum + (to - from), 0),
  );
  const total = durations.reduce((sum, d) => sum + d, 0);
  return slices.map((s, i) => ({
    customer: s.customer ?? "Unresolved",
    percent: total > 0 ? Math.round((durations[i] / total) * 100) : 0,
  }));
}

function splitLineText(parts: Part[]): string {
  return parts.length === 1
    ? parts[0].customer
    : parts.map((p) => `${p.customer} ${p.percent}%`).join(" · ");
}

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

/** The quiet, non-editing line: "Sjúkra 70% · MMS 30% · auto" plus a
 * compact two-colour bar when the block is actually split. */
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
      <span>
        {splitLineText(parts)} · {ORIGIN_LABELS[origin]}
      </span>
      {parts.length > 1 && (
        <div
          aria-hidden="true"
          style={{ display: "flex", height: 4, borderRadius: 2, overflow: "hidden", marginTop: 2 }}
        >
          {parts.map((p, i) => (
            <div
              key={p.customer}
              style={{
                width: `${p.percent}%`,
                background: i % 2 === 0 ? "var(--sage)" : "var(--amber)",
              }}
            />
          ))}
        </div>
      )}
      <button type="button" className="action-btn" onClick={onChange}>
        Change
      </button>
    </div>
  );
}

function SplitEditorRow({
  row,
  onPercent,
  onRemove,
}: {
  row: Row;
  onPercent: (percent: number) => void;
  onRemove: () => void;
}) {
  return (
    <div className="reg-row">
      <span>{row.customer}</span>
      <input
        className="reg-input"
        type="number"
        step={5}
        min={0}
        max={100}
        value={row.percent}
        aria-label={`${row.customer} share percent`}
        onChange={(e) => onPercent(Number(e.target.value))}
      />
      <button
        type="button"
        className="icon-btn"
        aria-label={`Remove ${row.customer}`}
        onClick={onRemove}
      >
        ×
      </button>
    </div>
  );
}

interface EditorProps {
  rows: Row[];
  customerOptions: string[];
  origin: SplitOrigin;
  error: string | null;
  isPending: boolean;
  onUpdateRow: (index: number, percent: number) => void;
  onRemoveRow: (index: number) => void;
  onAddRow: (customer: string) => void;
  onSplitEvenly: () => void;
  onReset: () => void;
  onCancel: () => void;
  onSave: () => void;
}

/** One row per customer with a % input, an "Add customer" select sourced
 * from `fetchTenants` (the only customer-name source this module may call —
 * see the T008 contract), and a live total that gates Save. */
function SplitEditor({
  rows,
  customerOptions,
  origin,
  error,
  isPending,
  onUpdateRow,
  onRemoveRow,
  onAddRow,
  onSplitEvenly,
  onReset,
  onCancel,
  onSave,
}: EditorProps) {
  const total = rows.reduce((sum, r) => sum + r.percent, 0);
  const canSave = rows.length > 0 && rows.every((r) => r.customer) && total === 100;
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
          key={`${row.customer}-${i}`}
          row={row}
          onPercent={(percent) => onUpdateRow(i, percent)}
          onRemove={() => onRemoveRow(i)}
        />
      ))}

      <select
        className="reg-input"
        aria-label="Add customer"
        value=""
        onChange={(e) => onAddRow(e.target.value)}
      >
        <option value="">Add customer…</option>
        {addableCustomers.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>

      <p className={total === 100 ? "settings-hint" : "export-error"}>Total: {total}%</p>
      {total !== 100 && (
        <p className="export-error" role="alert">
          Shares must add up to 100%
        </p>
      )}
      {error && (
        <p className="export-error" role="alert">
          {error}
        </p>
      )}

      <div className="reg-actions">
        <button type="button" className="action-btn" onClick={onSplitEvenly}>
          Split evenly
        </button>
        {origin === "manual" && (
          <button type="button" className="action-btn" disabled={isPending} onClick={onReset}>
            Reset to auto
          </button>
        )}
        <button type="button" className="action-btn" onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="action-btn primary" disabled={!canSave || isPending} onClick={onSave}>
          Save
        </button>
      </div>
    </div>
  );
}

/** Loads the block's slices once on mount — the only fetch the card list
 * itself pays for. */
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

/** "Add customer" needs real names — deferred to editor-open so the card
 * list itself never pays for it. */
function useCustomerOptions(editing: boolean): string[] {
  const [options, setOptions] = useState<string[]>([]);
  useEffect(() => {
    if (!editing) return;
    let alive = true;
    fetchTenants().then((r) => {
      if (!alive || !r.ok) return;
      const names = new Set<string>();
      for (const t of r.data) {
        if (t.customer) names.add(t.customer);
      }
      setOptions([...names]);
    });
    return () => {
      alive = false;
    };
  }, [editing]);
  return options;
}

/** State + mutations behind the card — split from the component so that
 * function stays under the size gate (mirrors BillingTenantSection's
 * `useTenantList`). */
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
    setRows(parts.map((p) => ({ customer: p.customer, percent: p.percent })));
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
      origin={origin}
      error={s.error}
      isPending={s.isPending}
      onUpdateRow={(index, percent) =>
        s.setRows((rs) => rs.map((r, i) => (i === index ? { ...r, percent } : r)))
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
