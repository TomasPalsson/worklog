"use client";

// Settings for the billing export — the registry that turns a work folder
// into a Viðskiptamaður and a Verkefni. Editable here so there is never a
// config file to open.
//
// Two tables, and one shortcut that does the real work: "folders seen
// recently with no mapping". Remembering which folders you worked in is
// the tedious part, so the panel lists them with an event count and lets
// you map one in a click instead of typing a name from memory.
//
// Rows save individually rather than behind one big Save button, because
// each row is an independent fact and a half-finished row shouldn't block
// saving a finished one.

import { useCallback, useEffect, useId, useState } from "react";
import { Loader2, Plus, Save, Trash2, Wallet, X } from "lucide-react";

import {
  deleteBillingCustomer,
  deleteBillingFolder,
  fetchBillingRegistry,
  saveBillingCustomer,
  saveBillingFolder,
} from "@/app/actions";
import { toast } from "@/lib/toast";
import type { BillingCustomer, BillingFolderMap, UnmappedFolder } from "@/lib/types";

interface Props {
  day: string;
}

/** A folder row being edited. `id` absent = not saved yet. */
type FolderDraft = BillingFolderMap & { key: string };
type CustomerDraft = BillingCustomer & { key: string };

/** Sentinel for "shared folder — resolve the customer from text". */
const SHARED = "__shared__";

let seq = 0;
const nextKey = () => `new-${seq++}`;

export function BillingRegistryPanel({ day }: Props) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [customers, setCustomers] = useState<CustomerDraft[]>([]);
  const [folders, setFolders] = useState<FolderDraft[]>([]);
  const [unmapped, setUnmapped] = useState<UnmappedFolder[]>([]);
  const titleId = useId();

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
    if (open) void load();
  }, [open, load]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && busy === null) setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, busy]);

  /** Run one row mutation, then refresh so ids/ordering stay truthful. */
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

  // ───────────────────────────── customers ─────────────────────────────

  function addCustomer() {
    setCustomers((cs) => [...cs, { key: nextKey(), name: "", aliases: [] }]);
  }

  function patchCustomer(key: string, patch: Partial<CustomerDraft>) {
    setCustomers((cs) => cs.map((c) => (c.key === key ? { ...c, ...patch } : c)));
  }

  // ──────────────────────────── folder pins ────────────────────────────

  function addFolder(folder = "") {
    setFolders((fs) => [
      ...fs,
      { key: nextKey(), folder, customer: null, verkefni: null, billable: true },
    ]);
  }

  function patchFolder(key: string, patch: Partial<FolderDraft>) {
    setFolders((fs) => fs.map((f) => (f.key === key ? { ...f, ...patch } : f)));
  }

  return (
    <>
      <button
        type="button"
        className="theme-toggle"
        onClick={() => setOpen(true)}
        aria-label="Open billing registry"
        title="Billing registry — customers and folder mappings"
      >
        <Wallet size={15} strokeWidth={1.75} />
      </button>

      {open && (
        <div
          className="settings-overlay"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget && busy === null) setOpen(false);
          }}
        >
          <div
            className="settings-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
          >
            <header className="settings-header">
              <h2 id={titleId}>Billing registry</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="Close billing registry"
                disabled={busy !== null}
                onClick={() => setOpen(false)}
              >
                <X size={16} />
              </button>
            </header>

            {loading ? (
              <div className="settings-loading">
                <Loader2 className="spin" size={20} />
                <span>Loading registry…</span>
              </div>
            ) : error ? (
              <div className="settings-body">
                <section className="settings-section">
                  <p className="export-error" role="alert">
                    Couldn&apos;t load the registry — {error}
                  </p>
                  <p className="export-hint">Is the worklog daemon running?</p>
                  <button type="button" className="action-btn" onClick={() => void load()}>
                    Retry
                  </button>
                </section>
              </div>
            ) : (
              <div className="settings-body">
                {unmapped.length > 0 && (
                  <section className="settings-section">
                    <h3>Folders seen recently with no mapping</h3>
                    <p className="settings-hint">
                      Work folders from the last 30 days that the export can&apos;t
                      resolve yet. Click one to start a mapping for it.
                    </p>
                    <div className="reg-chips">
                      {unmapped.map((u) => (
                        <button
                          key={u.folder}
                          type="button"
                          className="reg-chip"
                          title={`${u.events} events — click to map`}
                          onClick={() => addFolder(u.folder)}
                        >
                          <Plus size={11} />
                          {u.folder}
                          <span className="reg-chip-count">{u.events}</span>
                        </button>
                      ))}
                    </div>
                  </section>
                )}

                <section className="settings-section">
                  <h3>Folder mappings</h3>
                  <p className="settings-hint">
                    A folder pinned to a customer resolves without reading any
                    text. Leave the customer as <em>shared</em> when the folder
                    serves several customers — then each line&apos;s customer is
                    matched from its Jira ticket or description. Leave{" "}
                    <em>Verkefni</em> blank to always pick it in the form.
                  </p>

                  {folders.map((f) => (
                    <div key={f.key} className="reg-row">
                      <input
                        className="reg-input reg-folder"
                        placeholder="folder (e.g. sjukra)"
                        aria-label="Work folder"
                        value={f.folder}
                        onChange={(e) => patchFolder(f.key, { folder: e.target.value })}
                      />
                      <select
                        className="reg-input"
                        aria-label="Customer"
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
                        placeholder="Verkefni (blank = pick in form)"
                        aria-label="Verkefni"
                        value={f.verkefni ?? ""}
                        onChange={(e) =>
                          patchFolder(f.key, { verkefni: e.target.value || null })
                        }
                      />
                      <label className="reg-check" title="Reikningshæft">
                        <input
                          type="checkbox"
                          checked={f.billable}
                          onChange={(e) =>
                            patchFolder(f.key, { billable: e.target.checked })
                          }
                        />
                        Reikn.
                      </label>
                      <button
                        type="button"
                        className="icon-btn"
                        aria-label={`Save mapping for ${f.folder || "new folder"}`}
                        disabled={busy !== null || f.folder.trim() === ""}
                        onClick={() =>
                          void mutate(f.key, `Saved ${f.folder}`, () =>
                            saveBillingFolder(
                              {
                                id: f.id,
                                folder: f.folder,
                                customer: f.customer,
                                verkefni: f.verkefni,
                                billable: f.billable,
                              },
                              day,
                            ),
                          )
                        }
                      >
                        {busy === f.key ? (
                          <Loader2 className="spin" size={14} />
                        ) : (
                          <Save size={14} />
                        )}
                      </button>
                      <button
                        type="button"
                        className="icon-btn"
                        aria-label={`Delete mapping for ${f.folder || "new folder"}`}
                        disabled={busy !== null}
                        onClick={() => {
                          // Unsaved row: drop it locally, no round trip.
                          if (f.id == null) {
                            setFolders((fs) => fs.filter((x) => x.key !== f.key));
                            return;
                          }
                          void mutate(f.key, `Deleted ${f.folder}`, () =>
                            deleteBillingFolder(f.id as number, day),
                          );
                        }}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  ))}

                  <button type="button" className="action-btn" onClick={() => addFolder()}>
                    <Plus size={14} />
                    Add mapping
                  </button>
                </section>

                <section className="settings-section">
                  <h3>Customers</h3>
                  <p className="settings-hint">
                    Aliases are matched against a block&apos;s Jira ticket summary
                    and description to resolve shared folders. Comma- or
                    newline-separated; the name itself always matches. Short
                    aliases are matched on word boundaries, so{" "}
                    <code>RU</code> won&apos;t fire inside another word.
                  </p>

                  {customers.map((c) => (
                    <div key={c.key} className="reg-row">
                      <input
                        className="reg-input reg-folder"
                        placeholder="Customer name"
                        aria-label="Customer name"
                        value={c.name}
                        onChange={(e) => patchCustomer(c.key, { name: e.target.value })}
                      />
                      <input
                        className="reg-input reg-aliases"
                        placeholder="aliases, comma separated"
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
                      <button
                        type="button"
                        className="icon-btn"
                        aria-label={`Save customer ${c.name || "new"}`}
                        disabled={busy !== null || c.name.trim() === ""}
                        onClick={() =>
                          void mutate(c.key, `Saved ${c.name}`, () =>
                            saveBillingCustomer(
                              { id: c.id, name: c.name, aliases: c.aliases },
                              day,
                            ),
                          )
                        }
                      >
                        {busy === c.key ? (
                          <Loader2 className="spin" size={14} />
                        ) : (
                          <Save size={14} />
                        )}
                      </button>
                      <button
                        type="button"
                        className="icon-btn"
                        aria-label={`Delete customer ${c.name || "new"}`}
                        disabled={busy !== null}
                        onClick={() => {
                          if (c.id == null) {
                            setCustomers((cs) => cs.filter((x) => x.key !== c.key));
                            return;
                          }
                          void mutate(c.key, `Deleted ${c.name}`, () =>
                            deleteBillingCustomer(c.id as number, day),
                          );
                        }}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  ))}

                  <button type="button" className="action-btn" onClick={addCustomer}>
                    <Plus size={14} />
                    Add customer
                  </button>
                </section>
              </div>
            )}

            <footer className="settings-footer">
              <button
                type="button"
                className="action-btn"
                disabled={busy !== null}
                onClick={() => setOpen(false)}
              >
                Close
              </button>
            </footer>
          </div>
        </div>
      )}
    </>
  );
}
