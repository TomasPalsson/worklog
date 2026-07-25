"use client";

// Billing export panel — turns a reviewed day into the line items the
// external invoicing system charges customers from.
//
// Deliberately thin: the daemon pre-renders text/CSV/JSON (the Rust
// `billing` module is the single source of truth for grouping, the
// overlap-safe hour union, and half-hour rounding), so this component
// only displays rows, puts the pre-rendered text on the clipboard, and
// hands the pre-rendered files to the browser. No billing arithmetic
// happens here — that's what kept the old Tempo-era rounding mirror in
// `format.ts` drifting.

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Check, Copy, Download, FileSpreadsheet, Loader2, Receipt, X } from "lucide-react";

import { exportBilling, markExported } from "@/app/actions";
import {
  downloadText,
  exportFilename,
  exportMime,
  formatExportHours,
  totalBilledHours,
} from "@/lib/export";
import { toast } from "@/lib/toast";
import type { ExportResponse } from "@/lib/types";

interface Props {
  day: string;
}

export function ExportPanel({ day }: Props) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [data, setData] = useState<ExportResponse | null>(null);
  // Inline (not toast) so a failed open leaves the panel usable with a
  // Retry — a toast would vanish and leave an empty dialog behind.
  const [error, setError] = useState<string | null>(null);
  const [marking, setMarking] = useState(false);
  const titleId = useId();
  const textRef = useRef<HTMLPreElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    const r = await exportBilling(day);
    setLoading(false);
    if (!r.ok) {
      setError(r.error);
      return;
    }
    setData(r.data);
  }, [day]);

  // Fetch on every open — blocks may have been edited since last time.
  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  // Escape closes, but never mid-write.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !marking) setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, marking]);

  async function onCopy() {
    if (!data) return;
    try {
      await navigator.clipboard.writeText(data.rendered.text);
      toast.ok("Billing lines copied");
    } catch (e) {
      // Clipboard can be blocked by permissions or a non-secure origin.
      // The raw text stays selectable in the panel as the fallback.
      toast.error(
        `Couldn't copy — ${(e as Error).message}. Select the text below instead.`,
      );
      textRef.current?.focus();
    }
  }

  function onDownload(format: "csv" | "json") {
    if (!data) return;
    try {
      downloadText(
        exportFilename(day, format),
        exportMime(format),
        format === "csv" ? data.rendered.csv : data.rendered.json,
      );
      toast.ok(`Downloaded ${exportFilename(day, format)}`);
    } catch (e) {
      toast.error(`Download failed — ${(e as Error).message}`);
    }
  }

  async function onMark() {
    setMarking(true);
    const r = await markExported(day);
    setMarking(false);
    if (!r.ok) {
      toast.error(`Couldn't mark exported — ${r.error}`);
      return;
    }
    toast.ok(
      r.data.marked > 0
        ? `Marked ${r.data.marked} block${r.data.marked === 1 ? "" : "s"} exported`
        : "Already exported — nothing to mark",
    );
    // Refresh so the "already exported" stamp reflects the new state.
    await load();
  }

  const rows = data?.rows ?? [];
  const total = totalBilledHours(rows);

  return (
    <>
      <button
        type="button"
        className="theme-toggle"
        onClick={() => setOpen(true)}
        aria-label="Open billing export"
        title="Billing export — repo · description · hours · type"
      >
        <Receipt size={15} strokeWidth={1.75} />
      </button>

      {open && (
        <div
          className="settings-overlay"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget && !marking) setOpen(false);
          }}
        >
          <div
            className="settings-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
          >
            <header className="settings-header">
              <h2 id={titleId}>Billing export · {day}</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="Close billing export"
                disabled={marking}
                onClick={() => setOpen(false)}
              >
                <X size={16} />
              </button>
            </header>

            {loading ? (
              <div className="settings-loading">
                <Loader2 className="spin" size={20} />
                <span>Computing export…</span>
              </div>
            ) : error ? (
              <div className="settings-body">
                <section className="settings-section">
                  <p className="export-error" role="alert">
                    Couldn&apos;t load the export — {error}
                  </p>
                  <p className="export-hint">
                    Is the worklog daemon running?
                  </p>
                  <button type="button" className="action-btn" onClick={() => void load()}>
                    Retry
                  </button>
                </section>
              </div>
            ) : rows.length === 0 ? (
              <div className="settings-body">
                <section className="settings-section">
                  <p className="export-hint">No blocks to export for this day.</p>
                </section>
              </div>
            ) : (
              <div className="settings-body">
                {data?.exported_at && (
                  <p className="export-stamp">
                    <Check size={13} /> Already exported{" "}
                    {new Date(data.exported_at).toLocaleString()}
                  </p>
                )}

                <section className="settings-section">
                  <h3>
                    {rows.length} line item{rows.length === 1 ? "" : "s"} ·{" "}
                    {formatExportHours(total)} hrs
                  </h3>
                  <div className="export-table-wrap">
                    <table className="export-table">
                      <thead>
                        <tr>
                          <th scope="col">repo</th>
                          <th scope="col">description</th>
                          <th scope="col">time</th>
                          <th scope="col">type</th>
                        </tr>
                      </thead>
                      <tbody>
                        {rows.map((r, i) => (
                          <tr key={`${r.repo}-${r.kind}-${i}`}>
                            <td className="export-repo">{r.repo}</td>
                            <td>{r.description}</td>
                            <td className="export-hours">
                              {formatExportHours(r.hours)} hrs
                            </td>
                            <td>
                              <span className={`export-kind kind-${r.kind.toLowerCase()}`}>
                                {r.kind}
                              </span>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </section>

                <section className="settings-section">
                  <h3>Copy-paste</h3>
                  {/* tabIndex so the clipboard-blocked fallback can focus it. */}
                  <pre ref={textRef} tabIndex={-1} className="export-text">
                    {data?.rendered.text}
                  </pre>
                </section>
              </div>
            )}

            <footer className="settings-footer">
              <button
                type="button"
                className="action-btn"
                disabled={!data || rows.length === 0}
                onClick={() => void onCopy()}
                title="Copy the line items as text"
              >
                <Copy size={14} />
                Copy
              </button>
              <button
                type="button"
                className="action-btn"
                disabled={!data || rows.length === 0}
                onClick={() => onDownload("csv")}
                title="Download as CSV"
              >
                <FileSpreadsheet size={14} />
                CSV
              </button>
              <button
                type="button"
                className="action-btn"
                disabled={!data || rows.length === 0}
                onClick={() => onDownload("json")}
                title="Download as JSON"
              >
                <Download size={14} />
                JSON
              </button>
              <button
                type="button"
                className="action-btn"
                disabled={!data || rows.length === 0 || marking}
                aria-busy={marking || undefined}
                onClick={() => void onMark()}
                title="Stamp these blocks as exported (guards against double-billing; lets purge retire them later)"
              >
                {marking ? <Loader2 className="spin" size={14} /> : <Check size={14} />}
                {marking ? "Marking…" : "Mark exported"}
              </button>
            </footer>
          </div>
        </div>
      )}
    </>
  );
}
