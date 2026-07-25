"use client";

// The billing registry — what turns a work folder into a Viðskiptamaður
// and a Verkefni. A full page rather than a modal: it is two tables plus a
// discovery list, which is more than a dialog can show without cramping.
//
// The shortcut that does the real work is "folders seen recently with no
// mapping": remembering which folders you worked in is the tedious part, so
// they are listed with an event count and one click starts a prefilled row.
//
// Rows save individually rather than behind one Save button — each row is
// an independent fact, and a half-finished row must not block saving a
// finished one.

import { useCallback, useEffect, useState } from "react";
import { Loader2, Plus, Save, Trash2 } from "lucide-react";

import {
  deleteBillingCustomer,
  deleteBillingFolder,
  fetchBillingRegistry,
  saveBillingCustomer,
  saveBillingFolder,
} from "@/app/actions";
import { toast } from "@/lib/toast";
import type { BillingCustomer, BillingFolderMap, UnmappedFolder } from "@/lib/types";

type FolderDraft = BillingFolderMap & { key: string };
type CustomerDraft = BillingCustomer & { key: string };

/** Sentinel for "shared folder — resolve the customer from text". */
const SHARED = "__shared__";

let seq = 0;
const nextKey = () => `new-${seq++}`;

export function BillingRegistry() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [customers, setCustomers] = useState<CustomerDraft[]>([]);
  const [folders, setFolders] = useState<FolderDraft[]>([]);
  const [unmapped, setUnmapped] = useState<UnmappedFolder[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    const r = await fetchBillingRegistry();
    setLoading(false);
    if (!r.ok) {
      setError(r.error);
      return;
    }
    setCustomers(r.data.customers.map((c) => ({ ...c, key: `c${c.id}` })));
    setFolders(r.data.folders.map((f) => ({ ...f, key: `f${f.id}` })));
    setUnmapped(r.data.unmapped);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  /** Run one row mutation, then refresh so ids and ordering stay truthful. */
  async function mutate(
    key: string,
    label: string,
    fn: () => Promise<{ ok: true } | { ok: false; error: string }>,
  ) {
    setBusy(key);
    const r = await fn();
    setBusy(null);
    if (!r.ok) {
      toast.error(`${label} failed — ${r.error}`);
      return;
    }
    toast.ok(label);
    await load();
  }

  function addFolder(folder = "") {
    setFolders((fs) => [
      ...fs,
      { key: nextKey(), folder, customer: null, verkefni: null, billable: true },
    ]);
  }

  function patchFolder(key: string, patch: Partial<FolderDraft>) {
    setFolders((fs) => fs.map((f) => (f.key === key ? { ...f, ...patch } : f)));
  }

  function patchCustomer(key: string, patch: Partial<CustomerDraft>) {
    setCustomers((cs) => cs.map((c) => (c.key === key ? { ...c, ...patch } : c)));
  }

  if (loading) {
    return (
      <div className="settings-loading">
        <Loader2 className="spin" size={20} />
        <span>Loading registry…</span>
      </div>
    );
  }

  if (error) {
    return (
      <section className="reg-section">
        <p className="export-error" role="alert">
          Couldn&apos;t load the registry — {error}
        </p>
        <p className="export-hint">Is the worklog daemon running?</p>
        <button type="button" className="action-btn" onClick={() => void load()}>
          Retry
        </button>
      </section>
    );
  }

  return (
    <>
      {unmapped.length > 0 && (
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
                className="reg-chip"
                data-tip={`${u.events.toLocaleString()} events — click to map`}
                onClick={() => addFolder(u.folder)}
              >
                <Plus size={11} />
                {u.folder}
                <span className="reg-chip-count">{u.events.toLocaleString()}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      <section className="reg-section">
        <h2>Folder mappings</h2>
        <p className="reg-lede">
          A folder pinned to a customer resolves without reading any text.
          Leave the customer as <em>shared</em> when the folder serves several
          customers — then each line&apos;s customer is matched from its Jira
          ticket or description. Leave <em>Verkefni</em> blank to always pick
          it in the form; it is never guessed.
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
            <div key={f.key} className="reg-row reg-row-folder" role="row">
              <input
                className="reg-input reg-mono"
                placeholder="e.g. sjukra"
                aria-label="Work folder"
                value={f.folder}
                onChange={(e) => patchFolder(f.key, { folder: e.target.value })}
              />
              <select
                className="reg-input"
                aria-label="Viðskiptamaður"
                value={f.customer ?? SHARED}
                onChange={(e) =>
                  patchFolder(f.key, {
                    customer: e.target.value === SHARED ? null : e.target.value,
                  })
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
                value={f.verkefni ?? ""}
                onChange={(e) => patchFolder(f.key, { verkefni: e.target.value || null })}
              />
              <label
                className="reg-check"
                data-tip={f.billable ? "Reikningshæft" : "Óreikningshæft"}
              >
                <input
                  type="checkbox"
                  checked={f.billable}
                  aria-label="Reikningshæft"
                  onChange={(e) => patchFolder(f.key, { billable: e.target.checked })}
                />
              </label>
              <span className="reg-actions">
                <button
                  type="button"
                  className="icon-btn"
                  data-tip="Save this mapping"
                  aria-label={`Save mapping for ${f.folder || "new folder"}`}
                  disabled={busy !== null || f.folder.trim() === ""}
                  onClick={() =>
                    void mutate(f.key, `Saved ${f.folder}`, () =>
                      saveBillingFolder({
                        id: f.id,
                        folder: f.folder,
                        customer: f.customer,
                        verkefni: f.verkefni,
                        billable: f.billable,
                      }),
                    )
                  }
                >
                  {busy === f.key ? <Loader2 className="spin" size={14} /> : <Save size={14} />}
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  data-tip="Delete this mapping"
                  aria-label={`Delete mapping for ${f.folder || "new folder"}`}
                  disabled={busy !== null}
                  onClick={() => {
                    // Unsaved row: drop it locally, no round trip.
                    if (f.id == null) {
                      setFolders((fs) => fs.filter((x) => x.key !== f.key));
                      return;
                    }
                    void mutate(f.key, `Deleted ${f.folder}`, () =>
                      deleteBillingFolder(f.id as number),
                    );
                  }}
                >
                  <Trash2 size={14} />
                </button>
              </span>
            </div>
          ))}
        </div>

        <button
          type="button"
          className="action-btn"
          data-tip="Add an empty folder mapping"
          onClick={() => addFolder()}
        >
          <Plus size={14} />
          Add mapping
        </button>
      </section>

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
            <div key={c.key} className="reg-row reg-row-customer" role="row">
              <input
                className="reg-input"
                placeholder="Customer name"
                aria-label="Customer name"
                value={c.name}
                onChange={(e) => patchCustomer(c.key, { name: e.target.value })}
              />
              <input
                className="reg-input"
                placeholder="e.g. Sjukra, Sjúkratryggingar"
                aria-label="Aliases"
                value={c.aliases.join(", ")}
                onChange={(e) =>
                  patchCustomer(c.key, {
                    aliases: e.target.value
                      .split(",")
                      .map((a) => a.trim())
                      .filter((a) => a !== ""),
                  })
                }
              />
              <span className="reg-actions">
                <button
                  type="button"
                  className="icon-btn"
                  data-tip="Save this customer"
                  aria-label={`Save customer ${c.name || "new"}`}
                  disabled={busy !== null || c.name.trim() === ""}
                  onClick={() =>
                    void mutate(c.key, `Saved ${c.name}`, () =>
                      saveBillingCustomer({
                        id: c.id,
                        name: c.name,
                        aliases: c.aliases,
                      }),
                    )
                  }
                >
                  {busy === c.key ? <Loader2 className="spin" size={14} /> : <Save size={14} />}
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  data-tip="Delete this customer"
                  aria-label={`Delete customer ${c.name || "new"}`}
                  disabled={busy !== null}
                  onClick={() => {
                    if (c.id == null) {
                      setCustomers((cs) => cs.filter((x) => x.key !== c.key));
                      return;
                    }
                    void mutate(c.key, `Deleted ${c.name}`, () =>
                      deleteBillingCustomer(c.id as number),
                    );
                  }}
                >
                  <Trash2 size={14} />
                </button>
              </span>
            </div>
          ))}
        </div>

        <button
          type="button"
          className="action-btn"
          data-tip="Add an empty customer"
          onClick={() =>
            setCustomers((cs) => [...cs, { key: nextKey(), name: "", aliases: [] }])
          }
        >
          <Plus size={14} />
          Add customer
        </button>
      </section>
    </>
  );
}
