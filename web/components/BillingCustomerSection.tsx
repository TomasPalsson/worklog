"use client";

// Customer table for the Billing registry. Split out of
// BillingRegistry.tsx to keep that file under the size gate — no state of
// its own; the parent (useBillingRegistry) owns customers and mutations.

import { Loader2, Plus, Save, Trash2 } from "lucide-react";
import type { CustomerDraft } from "@/lib/billingRegistryDrafts";
import { useDeildir } from "@/lib/useBillingRegistry";
import { BillingDeildir } from "./BillingDeildir";

function CustomerActions({
  customer,
  busy,
  onSave,
  onDelete,
}: {
  customer: CustomerDraft;
  busy: string | null;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <span className="reg-actions">
      <button
        type="button"
        className="icon-btn"
        data-tip="Save this customer"
        aria-label={`Save customer ${customer.name || "new"}`}
        disabled={busy !== null || customer.name.trim() === ""}
        onClick={onSave}
      >
        {busy === customer.key ? <Loader2 className="spin" size={14} /> : <Save size={14} />}
      </button>
      <button
        type="button"
        className="icon-btn"
        data-tip="Delete this customer"
        aria-label={`Delete customer ${customer.name || "new"}`}
        disabled={busy !== null}
        onClick={onDelete}
      >
        <Trash2 size={14} />
      </button>
    </span>
  );
}

function CustomerRow({
  customer,
  busy,
  isNew,
  onPatch,
  onSave,
  onDelete,
}: {
  customer: CustomerDraft;
  busy: string | null;
  isNew: boolean;
  onPatch: (patch: Partial<CustomerDraft>) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      data-row-key={customer.key}
      className={`reg-row reg-row-customer${isNew ? " is-new" : ""}`}
      role="row"
    >
      <input
        className="reg-input"
        placeholder="Customer name"
        aria-label="Customer name"
        value={customer.name}
        onChange={(e) => onPatch({ name: e.target.value })}
      />
      <input
        className="reg-input"
        placeholder="e.g. Sjukra, Sjúkratryggingar"
        aria-label="Aliases"
        value={customer.aliases.join(", ")}
        onChange={(e) =>
          onPatch({
            aliases: e.target.value
              .split(",")
              .map((a) => a.trim())
              .filter((a) => a !== ""),
          })
        }
      />
      <CustomerActions customer={customer} busy={busy} onSave={onSave} onDelete={onDelete} />
    </div>
  );
}

export function CustomerSection({
  customers,
  busy,
  justAdded,
  onPatch,
  onSave,
  onDelete,
  onAdd,
}: {
  customers: CustomerDraft[];
  busy: string | null;
  justAdded: string | null;
  onPatch: (key: string, patch: Partial<CustomerDraft>) => void;
  onSave: (customer: CustomerDraft) => void;
  onDelete: (customer: CustomerDraft) => void;
  onAdd: () => void;
}) {
  const deildState = useDeildir();

  return (
    <section className="reg-section">
      <h2>Customers</h2>
      <p className="reg-lede">
        Aliases are matched against a block&apos;s Jira ticket summary and
        description to resolve shared folders. Comma-separated; the name
        itself always matches. Short aliases match on word boundaries, so{" "}
        <code>RU</code> won&apos;t fire inside another word. If two customers
        match one line, it&apos;s left blank rather than guessed.
      </p>

      <div className="reg-table" role="table">
        <div className="reg-head reg-row-customer" role="row">
          <span role="columnheader">Name</span>
          <span role="columnheader">Aliases</span>
          <span />
        </div>

        {customers.map((c) => (
          <div key={c.key}>
            <CustomerRow
              customer={c}
              busy={busy}
              isNew={justAdded === c.key}
              onPatch={(patch) => onPatch(c.key, patch)}
              onSave={() => onSave(c)}
              onDelete={() => onDelete(c)}
            />
            {c.name.trim() !== "" && (
              <BillingDeildir
                customer={c.name}
                deildir={deildState.deildir}
                busy={deildState.busy}
                justAdded={deildState.justAdded}
                onPatch={deildState.patchDeild}
                onSave={deildState.saveDeild}
                onDelete={deildState.deleteDeild}
                onAdd={deildState.addDeild}
              />
            )}
          </div>
        ))}
      </div>

      <button type="button" className="action-btn" data-tip="Add an empty customer" onClick={onAdd}>
        <Plus size={14} />
        Add customer
      </button>
    </section>
  );
}
