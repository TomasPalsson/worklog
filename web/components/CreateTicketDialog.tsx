"use client";

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Loader2, X } from "lucide-react";
import type { JiraProject, JiraTicket, TempoAccount } from "@/lib/types";
import { createTicket, fetchAccounts, fetchProjects } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  open: boolean;
  onClose: () => void;
  blockId: number;
  day: string;
  /** Called after the ticket is created and assigned to the block, so the
   * combobox can close itself and restore focus. */
  onCreated: (ticket: JiraTicket) => void;
}

/**
 * Modal for opening a brand-new Jira ticket from the review UI. The
 * account is the point: it's the Tempo account custom field that maps
 * the ticket's worklogs to a billable customer, so it's required. On
 * success the daemon creates the issue, sets the account, caches it, and
 * the action assigns it to the block in the same round-trip.
 *
 * Projects + accounts are fetched lazily the first time the dialog opens
 * and cached for the component's lifetime — both lists are small and
 * change rarely.
 */
export function CreateTicketDialog({ open, onClose, blockId, day, onCreated }: Props) {
  const [projects, setProjects] = useState<JiraProject[] | null>(null);
  const [accounts, setAccounts] = useState<TempoAccount[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const [projectKey, setProjectKey] = useState("");
  const [summary, setSummary] = useState("");
  const [accountId, setAccountId] = useState("");
  const [description, setDescription] = useState("");

  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const firstFieldRef = useRef<HTMLSelectElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    const [p, a] = await Promise.all([fetchProjects(), fetchAccounts()]);
    setLoading(false);
    if (!p.ok) {
      setLoadError(`Couldn't load projects — ${p.error}`);
      return;
    }
    if (!a.ok) {
      setLoadError(`Couldn't load accounts — ${a.error}`);
      return;
    }
    setProjects(p.data);
    setAccounts(a.data);
  }, []);

  // Fetch lists once, the first time the dialog is opened.
  useEffect(() => {
    if (open && projects === null && accounts === null && !loading) {
      load();
    }
  }, [open, projects, accounts, loading, load]);

  // Focus the first field when the lists are ready.
  useEffect(() => {
    if (open && projects && accounts) firstFieldRef.current?.focus();
  }, [open, projects, accounts]);

  // Escape closes (unless mid-submit).
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !submitting) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, submitting, onClose]);

  if (!open) return null;

  const canSubmit =
    projectKey.trim() !== "" &&
    summary.trim() !== "" &&
    accountId.trim() !== "" &&
    !submitting;

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    const r = await createTicket(
      {
        project_key: projectKey,
        summary: summary.trim(),
        account_id: accountId,
        description: description.trim() || undefined,
      },
      blockId,
      day,
    );
    setSubmitting(false);
    if (!r.ok) {
      toast.error(`Create ticket failed — ${r.error}`);
      return;
    }
    toast.ok(`Created ${r.data.key} and assigned to this block.`);
    // Reset the form so a subsequent open starts clean.
    setSummary("");
    setDescription("");
    onCreated(r.data);
  }

  return (
    <div
      className="settings-overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !submitting) onClose();
      }}
    >
      <div
        ref={dialogRef}
        className="settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className="settings-header">
          <h2 id={titleId}>New Jira ticket</h2>
          <button
            type="button"
            className="icon-btn"
            aria-label="Close"
            disabled={submitting}
            onClick={onClose}
          >
            <X size={16} />
          </button>
        </header>

        {loading || projects === null || accounts === null ? (
          <div className="settings-loading">
            {loadError ? (
              <span role="status">{loadError}</span>
            ) : (
              <>
                <Loader2 className="spin" size={20} />
                <span>Loading projects &amp; accounts…</span>
              </>
            )}
          </div>
        ) : (
          <form onSubmit={onSubmit}>
            <div className="settings-body">
              <section className="settings-section">
                <label className="settings-field">
                  <span>Project</span>
                  <select
                    ref={firstFieldRef}
                    value={projectKey}
                    onChange={(e) => setProjectKey(e.target.value)}
                    required
                  >
                    <option value="">Select a project…</option>
                    {projects.map((p) => (
                      <option key={p.key} value={p.key}>
                        {p.key} — {p.name}
                      </option>
                    ))}
                  </select>
                </label>

                <label className="settings-field" style={{ marginTop: 12 }}>
                  <span>Summary</span>
                  <input
                    type="text"
                    value={summary}
                    placeholder="Short imperative title"
                    autoComplete="off"
                    onChange={(e) => setSummary(e.target.value)}
                    required
                  />
                </label>

                <label className="settings-field" style={{ marginTop: 12 }}>
                  <span>
                    Account{" "}
                    <em className="settings-stored" title="Maps the ticket to a customer">
                      · customer mapping
                    </em>
                  </span>
                  <select
                    value={accountId}
                    onChange={(e) => setAccountId(e.target.value)}
                    required
                  >
                    <option value="">Select an account…</option>
                    {accounts.map((a) => (
                      <option key={a.id} value={String(a.id)}>
                        {a.name}
                        {a.customer ? ` — ${a.customer}` : ""} ({a.key})
                      </option>
                    ))}
                  </select>
                </label>

                <label className="settings-field" style={{ marginTop: 12 }}>
                  <span>Description (optional)</span>
                  <textarea
                    rows={3}
                    value={description}
                    spellCheck
                    onChange={(e) => setDescription(e.target.value)}
                  />
                </label>
                <p className="settings-hint" style={{ marginTop: 12 }}>
                  Issue type defaults to <code>Task</code>. The ticket is
                  created in Jira, the account is set so its time maps to the
                  customer, and it&rsquo;s assigned to this block.
                </p>
              </section>
            </div>

            <footer className="settings-footer">
              <button
                type="button"
                className="action-btn"
                disabled={submitting}
                onClick={onClose}
              >
                Cancel
              </button>
              <button type="submit" className="action-btn primary" disabled={!canSubmit}>
                {submitting ? <Loader2 className="spin" size={15} /> : null}
                {submitting ? "Creating…" : "Create & assign"}
              </button>
            </footer>
          </form>
        )}
      </div>
    </div>
  );
}
