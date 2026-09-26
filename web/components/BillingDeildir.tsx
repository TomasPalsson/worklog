"use client";

// The deildir list under one customer row (spec 006, Journey 1). Split out
// of BillingCustomerSection.tsx to keep that file under the size gate —
// no state of its own; `useDeildir` (web/lib/useBillingRegistry.ts) owns
// the deildir list and mutations.

import { Loader2, Plus, Save, Trash2 } from "lucide-react";
import type { DeildDraft } from "@/lib/useBillingRegistry";

function DeildRow({
  deild,
  busy,
  isNew,
  onPatch,
  onSave,
  onDelete,
}: {
  deild: DeildDraft;
  busy: string | null;
  isNew: boolean;
  onPatch: (patch: Partial<DeildDraft>) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      data-row-key={deild.key}
      className={`reg-row reg-row-customer${isNew ? " is-new" : ""}`}
      role="row"
    >
      <input
        className="reg-input"
        placeholder="Deild name"
        aria-label="Deild name"
        value={deild.name}
        onChange={(e) => onPatch({ name: e.target.value })}
      />
      <input
        className="reg-input"
        placeholder="e.g. rekstur, ops"
        aria-label="Deild keywords"
        value={deild.keywords.join(", ")}
        onChange={(e) =>
          onPatch({
            keywords: e.target.value
              .split(",")
              .map((k) => k.trim())
              .filter((k) => k !== ""),
          })
        }
      />
      <span className="reg-actions">
        <button
          type="button"
          className="icon-btn"
          data-tip="Save this deild"
          aria-label={`Save deild ${deild.name || "new"}`}
          disabled={busy !== null || deild.name.trim() === ""}
          onClick={onSave}
        >
          {busy === deild.key ? <Loader2 className="spin" size={14} /> : <Save size={14} />}
        </button>
        <button
          type="button"
          className="icon-btn"
          data-tip="Delete this deild"
          aria-label={`Delete deild ${deild.name || "new"}`}
          disabled={busy !== null}
          onClick={onDelete}
        >
          <Trash2 size={14} />
        </button>
      </span>
    </div>
  );
}

/** All deildir under one customer, plus an "Add deild" row. `deildir` is
 * the customer's full list (filtered by name) rather than pre-filtered,
 * so callers can share one `useDeildir()` instance across every customer. */
export function BillingDeildir({
  customer,
  deildir,
  busy,
  justAdded,
  onPatch,
  onSave,
  onDelete,
  onAdd,
}: {
  customer: string;
  deildir: DeildDraft[];
  busy: string | null;
  justAdded: string | null;
  onPatch: (key: string, patch: Partial<DeildDraft>) => void;
  onSave: (deild: DeildDraft) => void;
  onDelete: (deild: DeildDraft) => void;
  onAdd: (customer: string) => void;
}) {
  const rows = deildir.filter((d) => d.customer === customer);
  return (
    <div style={{ marginLeft: 16, marginBottom: 10 }}>
      {rows.map((d) => (
        <DeildRow
          key={d.key}
          deild={d}
          busy={busy}
          isNew={justAdded === d.key}
          onPatch={(patch) => onPatch(d.key, patch)}
          onSave={() => onSave(d)}
          onDelete={() => onDelete(d)}
        />
      ))}
      <button
        type="button"
        className="action-btn"
        data-tip={`Add a deild under ${customer}`}
        onClick={() => onAdd(customer)}
      >
        <Plus size={14} />
        Add deild
      </button>
    </div>
  );
}
