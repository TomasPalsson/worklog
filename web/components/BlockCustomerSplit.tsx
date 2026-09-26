"use client";

// Block customer split line + inline editor (spec 005 B17/FR-10/FR-14; spec
// 006 B4/B5/FR-04/FR-05) — rows of (customer, deild, %), a customer may
// repeat with a different deild. Available on every non-personal block; an
// empty or failed slices fetch just means nothing to show yet. Fetched
// lazily on mount so the day's card list never waits.

import type { SplitOrigin } from "@/lib/tenants";
import {
  type CustomerOptions,
  type Part,
  type Row,
  formatApprox,
  sliceParts,
  splitEvenly,
  splitLineText,
  totalSeconds,
  useCustomerSplit,
} from "@/lib/useCustomerSplit";

const ORIGIN_LABELS: Record<SplitOrigin, string> = {
  clues: "auto",
  manual: "set by you",
  fallback: "guess",
};

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
  deildOptions,
  approxSeconds,
  onSetCustomer,
  onSetDeild,
  onPercent,
  onRemove,
}: {
  row: Row;
  customerOptions: string[];
  deildOptions: string[];
  approxSeconds: number;
  onSetCustomer: (customer: string) => void;
  onSetDeild: (deild: string) => void;
  onPercent: (percent: number) => void;
  onRemove: () => void;
}) {
  const label = row.customer || "unresolved customer";
  // Distinguishes a repeated customer's rows (FR-04) as long as their
  // deildir differ; two blank-deild rows for the same customer share a
  // label — an edge case the Owner resolves by picking a deild first.
  const rowLabel = row.deild ? `${label} · ${row.deild}` : label;
  return (
    <div className="split-row">
      {row.customer ? (
        <span className="split-row-name">{row.customer}</span>
      ) : (
        <select
          className="reg-input split-row-name"
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
      <select
        className="reg-input split-row-deild-picker"
        aria-label={`${rowLabel} deild`}
        value={row.deild}
        onChange={(e) => onSetDeild(e.target.value)}
      >
        <option value="">No deild</option>
        {deildOptions.map((name) => (
          <option key={name} value={name}>{name}</option>
        ))}
      </select>
      <span className="split-row-pct">
        <input
          className="reg-input split-row-pct-input"
          type="number"
          step={5}
          min={0}
          max={100}
          value={row.percent}
          aria-label={`${rowLabel} share percent`}
          onChange={(e) => onPercent(Number(e.target.value))}
        />
        <span aria-hidden="true">%</span>
      </span>
      <span className="split-row-approx">≈ {formatApprox((approxSeconds * row.percent) / 100)}</span>
      <button type="button" className="icon-btn" aria-label={`Remove ${rowLabel}`} onClick={onRemove}>×</button>
    </div>
  );
}

interface EditorProps {
  rows: Row[];
  customerOptions: CustomerOptions;
  totalDurationSeconds: number;
  origin: SplitOrigin;
  error: string | null;
  isPending: boolean;
  onUpdateRow: (index: number, percent: number) => void;
  onSetRowCustomer: (index: number, customer: string) => void;
  onSetRowDeild: (index: number, deild: string) => void;
  onRemoveRow: (index: number) => void;
  onAddRow: (customer: string) => void;
  onSplitEvenly: () => void;
  onReset: () => void;
  onCancel: () => void;
  onSave: () => void;
}

/** One row per (customer, deild) — a customer may repeat (FR-04) — an "Add
 * customer" select sourced from the full registry (every customer, not
 * just already-linked tenants), and a live total that gates Save. */
function SplitEditor({
  rows,
  customerOptions,
  totalDurationSeconds,
  origin,
  error,
  isPending,
  onUpdateRow,
  onSetRowCustomer,
  onSetRowDeild,
  onRemoveRow,
  onAddRow,
  onSplitEvenly,
  onReset,
  onCancel,
  onSave,
}: EditorProps) {
  const total = rows.reduce((sum, r) => sum + r.percent, 0);
  const canSave = rows.length > 0 && rows.every((r) => r.customer !== "") && total === 100;

  return (
    <div
      className="block-clue-line customer-split-editor"
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
    >
      <div className="split-row split-head" aria-hidden="true">
        <span>Customer</span>
        <span>Deild</span>
        <span className="split-head-num">Share</span>
        <span className="split-head-num">Time</span>
        <span />
      </div>
      {rows.map((row, i) => (
        <SplitEditorRow
          key={i}
          row={row}
          customerOptions={customerOptions.customers}
          deildOptions={customerOptions.deildirByCustomer[row.customer] ?? []}
          approxSeconds={totalDurationSeconds}
          onSetCustomer={(customer) => onSetRowCustomer(i, customer)}
          onSetDeild={(deild) => onSetRowDeild(i, deild)}
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
        {customerOptions.customers.map((name) => (
          <option key={name} value={name}>{name}</option>
        ))}
      </select>

      <div className="split-editor-footer">
        <span className={total === 100 ? "split-total" : "split-total is-off"}>
          <span>Total: {total}%</span>
          {total !== 100 && <span role="alert">Shares must add up to 100%</span>}
        </span>
        <div className="reg-actions">
          <button type="button" className="action-btn" onClick={onSplitEvenly}>Split evenly</button>
          {origin === "manual" && (
            <button type="button" className="action-btn" disabled={isPending} onClick={onReset}>Reset to auto</button>
          )}
          <button type="button" className="action-btn" onClick={onCancel}>Cancel</button>
          <button type="button" className="action-btn primary" disabled={!canSave || isPending} onClick={onSave}>Save</button>
        </div>
      </div>
      {error && <p className="export-error split-error" role="alert">{error}</p>}
    </div>
  );
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
      onSetRowDeild={(index, deild) =>
        s.setRows((rs) => rs.map((r, i) => (i === index ? { ...r, deild } : r)))
      }
      onRemoveRow={(index) => s.setRows((rs) => rs.filter((_, i) => i !== index))}
      onAddRow={(customer) => {
        if (!customer) return;
        s.setRows((rs) => [...rs, { customer, deild: "", percent: 0 }]);
      }}
      onSplitEvenly={() => s.setRows((rs) => splitEvenly(rs))}
      onReset={s.reset}
      onCancel={s.closeEditor}
      onSave={s.save}
    />
  );
}
