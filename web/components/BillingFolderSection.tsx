"use client";

// Folder-mapping discovery + table for the Billing registry. Split out of
// BillingRegistry.tsx to keep that file under the size gate — no state of
// its own; the parent (useBillingRegistry) owns folders/customers and all
// mutations.

import { Check, Loader2, Plus, Save, Trash2 } from "lucide-react";
import type { CustomerDraft, FolderDraft } from "@/lib/billingRegistryDrafts";
import type { UnmappedFolder } from "@/lib/types";

const SHARED = "__shared__";

function UnmappedChips({
  unmapped,
  isQueued,
  onPick,
}: {
  unmapped: UnmappedFolder[];
  isQueued: (folder: string) => boolean;
  onPick: (folder: string) => void;
}) {
  if (unmapped.length === 0) return null;
  return (
    <section className="reg-section">
      <h2>Folders with no mapping</h2>
      <p className="reg-lede">
        Work folders seen in the last 30 days that the export can&apos;t
        resolve yet, busiest first. Click one to start a mapping for it.
      </p>
      <div className="reg-chips">
        {unmapped.map((u) => (
          <button
            key={u.folder}
            type="button"
            className={`reg-chip${isQueued(u.folder) ? " is-queued" : ""}`}
            data-tip={
              isQueued(u.folder)
                ? "Row started below — fill it in and save"
                : `${u.events.toLocaleString()} events — click to map`
            }
            onClick={() => onPick(u.folder)}
          >
            {isQueued(u.folder) ? <Check size={11} /> : <Plus size={11} />}
            {u.folder}
            <span className="reg-chip-count">{u.events.toLocaleString()}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

function FolderActions({
  folder,
  busy,
  onSave,
  onDelete,
}: {
  folder: FolderDraft;
  busy: string | null;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <span className="reg-actions">
      <button
        type="button"
        className="icon-btn"
        data-tip="Save this mapping"
        aria-label={`Save mapping for ${folder.folder || "new folder"}`}
        disabled={busy !== null || folder.folder.trim() === ""}
        onClick={onSave}
      >
        {busy === folder.key ? <Loader2 className="spin" size={14} /> : <Save size={14} />}
      </button>
      <button
        type="button"
        className="icon-btn"
        data-tip="Delete this mapping"
        aria-label={`Delete mapping for ${folder.folder || "new folder"}`}
        disabled={busy !== null}
        onClick={onDelete}
      >
        <Trash2 size={14} />
      </button>
    </span>
  );
}

function FolderRow({
  folder,
  customers,
  busy,
  isNew,
  onPatch,
  onSave,
  onDelete,
}: {
  folder: FolderDraft;
  customers: CustomerDraft[];
  busy: string | null;
  isNew: boolean;
  onPatch: (patch: Partial<FolderDraft>) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      data-row-key={folder.key}
      className={`reg-row reg-row-folder${isNew ? " is-new" : ""}`}
      role="row"
    >
      <input
        className="reg-input reg-mono"
        placeholder="e.g. sjukra"
        aria-label="Work folder"
        value={folder.folder}
        onChange={(e) => onPatch({ folder: e.target.value })}
      />
      <select
        className="reg-input"
        aria-label="Viðskiptamaður"
        value={folder.customer ?? SHARED}
        onChange={(e) =>
          onPatch({ customer: e.target.value === SHARED ? null : e.target.value })
        }
      >
        <option value={SHARED}>shared — from text</option>
        {customers
          .filter((c) => c.name.trim() !== "")
          .map((c) => (
            <option key={c.key} value={c.name}>
              {c.name}
            </option>
          ))}
      </select>
      <input
        className="reg-input"
        placeholder="blank = pick in form"
        aria-label="Verkefni"
        value={folder.verkefni ?? ""}
        onChange={(e) => onPatch({ verkefni: e.target.value || null })}
      />
      <label
        className="reg-check"
        data-tip={folder.billable ? "Reikningshæft" : "Óreikningshæft"}
      >
        <input
          type="checkbox"
          checked={folder.billable}
          aria-label="Reikningshæft"
          onChange={(e) => onPatch({ billable: e.target.checked })}
        />
      </label>
      <FolderActions folder={folder} busy={busy} onSave={onSave} onDelete={onDelete} />
    </div>
  );
}

export function FolderMappingsSection({
  unmapped,
  folders,
  customers,
  busy,
  justAdded,
  isQueued,
  onPickUnmapped,
  onPatch,
  onSave,
  onDelete,
  onAdd,
}: {
  unmapped: UnmappedFolder[];
  folders: FolderDraft[];
  customers: CustomerDraft[];
  busy: string | null;
  justAdded: string | null;
  isQueued: (folder: string) => boolean;
  onPickUnmapped: (folder: string) => void;
  onPatch: (key: string, patch: Partial<FolderDraft>) => void;
  onSave: (folder: FolderDraft) => void;
  onDelete: (folder: FolderDraft) => void;
  onAdd: () => void;
}) {
  return (
    <>
      <UnmappedChips unmapped={unmapped} isQueued={isQueued} onPick={onPickUnmapped} />

      <section className="reg-section">
        <h2>Folder mappings</h2>
        <p className="reg-lede">
          A folder pinned to a customer resolves without reading any text.
          Leave the customer as <em>shared</em> when the folder serves several
          customers — then each line&apos;s customer is matched from its Jira
          ticket or description. Leave <em>Verkefni</em> blank to always pick
          it in the form; it is never guessed. A row doesn&apos;t need a real
          folder on disk — name a project (e.g. &ldquo;AWS cert&rdquo;) to
          route and bill browser/Slack time that has no folder of its own.
        </p>

        <div className="reg-table" role="table">
          <div className="reg-head reg-row-folder" role="row">
            <span role="columnheader">Work folder</span>
            <span role="columnheader">Viðskiptamaður</span>
            <span role="columnheader">Verkefni (deild)</span>
            <span role="columnheader">Reikn.</span>
            <span />
          </div>

          {folders.map((f) => (
            <FolderRow
              key={f.key}
              folder={f}
              customers={customers}
              busy={busy}
              isNew={justAdded === f.key}
              onPatch={(patch) => onPatch(f.key, patch)}
              onSave={() => onSave(f)}
              onDelete={() => onDelete(f)}
            />
          ))}
        </div>

        <button
          type="button"
          className="action-btn"
          data-tip="Add an empty folder mapping"
          onClick={onAdd}
        >
          <Plus size={14} />
          Add mapping
        </button>
      </section>
    </>
  );
}
