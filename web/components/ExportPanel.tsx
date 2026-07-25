"use client";

// Billing export panel — walks the day's line items one at a time, in the
// invoicing form's own field order, so the user reads straight down while
// filling the real form in another tab.
//
// Why a wizard and not a table: each line is one submission of a form with
// eight fields, so the bottleneck is not seeing the whole day at once —
// it's not losing your place halfway through. Ticking a line off advances
// to the next and the dots show what's left.
//
// Deliberately thin: the daemon pre-renders text/CSV/JSON, so the Rust
// `billing` module stays the single source of truth for grouping, the
// overlap-safe hour union and half-hour rounding. Nothing here does
// billing arithmetic.

import { useCallback, useEffect, useId, useMemo, useState } from "react";
import {
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  FileSpreadsheet,
  Loader2,
  Receipt,
  TriangleAlert,
  X,
} from "lucide-react";

import { exportBilling, markExported } from "@/app/actions";
import {
  downloadText,
  exportFilename,
  exportMime,
  formatExportHours,
  formFields,
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
  // Inline rather than a toast: a failed open must leave the panel usable
  // with a Retry instead of an empty dialog behind a vanished message.
  const [error, setError] = useState<string | null>(null);
  const [marking, setMarking] = useState(false);
  const [index, setIndex] = useState(0);
  const [done, setDone] = useState<Set<number>>(new Set());
  const titleId = useId();

  const rows = useMemo(() => data?.rows ?? [], [data]);
  const current = rows[index];
  const fields = useMemo(() => (current ? formFields(current) : []), [current]);

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
    setIndex(0);
    setDone(new Set());
  }, [day]);

  // Refetch on every open — blocks may have been edited since last time.
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

  async function copyValue(label: string, value: string) {
    try {
      await navigator.clipboard.writeText(value);
      toast.ok(`${label} copied`);
    } catch (e) {
      // Blocked by permissions or a non-secure origin. The value stays
      // selectable on screen, so say so rather than failing silently.
      toast.error(`Couldn't copy ${label} — ${(e as Error).message}`);
    }
  }

  function markLineDone() {
    setDone((prev) => new Set(prev).add(index));
    // Advance to the next line still to do, so ticking off out of order
    // doesn't dump the user back onto something they already filed.
    const next = rows.findIndex((_, i) => i !== index && !done.has(i));
    if (next !== -1) setIndex(next);
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
    await load();
  }

  const total = totalBilledHours(rows);
  const missingCount = rows.filter((r) => r.customer === null || r.verkefni === null).length;
  // Lines whose Texti á reikning is a "Work in <folder>" fallback because the
  // day was never estimated. Without saying so, that reads as a bug.
  const undescribed = rows.filter((r) => r.needs_description).length;

  return (
    <>
      <button
        type="button"
        className="theme-toggle"
        onClick={() => setOpen(true)}
        aria-label="Open billing export"
        data-tip="Billing export"
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
              <h2 id={titleId}>
                Billing export · {day}
                {rows.length > 0 && (
                  <span className="export-counter">
                    line {index + 1} of {rows.length}
                  </span>
                )}
              </h2>
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
                  <p className="export-hint">Is the worklog daemon running?</p>
                  <button type="button" className="action-btn" onClick={() => void load()}>
                    Retry
                  </button>
                </section>
              </div>
            ) : rows.length === 0 ? (
              <div className="settings-body">
                <section className="settings-section">
                  <p className="export-hint">No billable work to export for this day.</p>
                </section>
              </div>
            ) : (
              <div className="settings-body">
                <p className="export-summary">
                  {rows.length} line{rows.length === 1 ? "" : "s"} ·{" "}
                  {formatExportHours(total)} hrs
                  {missingCount > 0 && (
                    <span className="export-missing-note">
                      <TriangleAlert size={12} /> {missingCount} need a pick
                    </span>
                  )}
                  {undescribed > 0 && (
                    <span
                      className="export-missing-note"
                      data-tip="Run “Estimate with Claude” to replace these with real descriptions"
                    >
                      <TriangleAlert size={12} /> {undescribed} need a description
                    </span>
                  )}
                  {data?.exported_at && (
                    <span className="export-stamp-inline">
                      <Check size={12} /> exported{" "}
                      {new Date(data.exported_at).toLocaleDateString()}
                    </span>
                  )}
                </p>

                {/* One line item, laid out in the invoicing form's order. */}
                <dl className={`export-form${done.has(index) ? " is-done" : ""}`}>
                  {fields.map((f) => (
                    <div
                      key={f.label}
                      className={`export-field${f.missing ? " is-missing" : ""}`}
                    >
                      <dt>{f.label}</dt>
                      <dd>
                        {f.missing ? (
                          <span className="export-pick">
                            <TriangleAlert size={12} /> pick in form
                          </span>
                        ) : (
                          <span className="export-value">{f.value}</span>
                        )}
                        {f.label === "Texti á reikning" && current?.needs_description && (
                          <span
                            className="export-fallback-tag"
                            data-tip="No description yet — this is a placeholder from the folder name"
                          >
                            not estimated
                          </span>
                        )}
                        {f.copyable && !f.missing && (
                          <button
                            type="button"
                            className="export-copy"
                            data-tip={`Copy ${f.label}`}
                            aria-label={`Copy ${f.label}`}
                            onClick={() => void copyValue(f.label, f.value)}
                          >
                            <Copy size={12} />
                          </button>
                        )}
                      </dd>
                    </div>
                  ))}
                </dl>

                {current?.ticket && (
                  <p className="export-hint export-context">
                    from {current.ticket} · {current.folder}
                  </p>
                )}

                <div className="export-nav">
                  <button
                    type="button"
                    className="action-btn"
                    disabled={index === 0}
                    data-tip="Previous line"
                    onClick={() => setIndex((i) => Math.max(0, i - 1))}
                  >
                    <ChevronLeft size={14} />
                    Prev
                  </button>
                  <button
                    type="button"
                    className="action-btn"
                    onClick={markLineDone}
                    data-tip="Tick off and go to the next outstanding line"
                  >
                    <Check size={14} />
                    {done.has(index) ? "Done" : "Done → next"}
                  </button>
                  <button
                    type="button"
                    className="action-btn"
                    disabled={index >= rows.length - 1}
                    data-tip="Next line"
                    onClick={() => setIndex((i) => Math.min(rows.length - 1, i + 1))}
                  >
                    Next
                    <ChevronRight size={14} />
                  </button>

                  <ol className="export-dots" aria-label="line progress">
                    {rows.map((_, i) => (
                      <li key={i}>
                        <button
                          type="button"
                          className={`export-dot${i === index ? " is-current" : ""}${
                            done.has(i) ? " is-done" : ""
                          }`}
                          aria-label={`Go to line ${i + 1}${done.has(i) ? " (done)" : ""}`}
                          aria-current={i === index || undefined}
                          onClick={() => setIndex(i)}
                        />
                      </li>
                    ))}
                  </ol>
                </div>
              </div>
            )}

            <footer className="settings-footer">
              <button
                type="button"
                className="action-btn"
                disabled={!data || rows.length === 0}
                onClick={() => onDownload("csv")}
                data-tip="Download the day as CSV"
              >
                <FileSpreadsheet size={14} />
                CSV
              </button>
              <button
                type="button"
                className="action-btn"
                disabled={!data || rows.length === 0}
                onClick={() => onDownload("json")}
                data-tip="Download the day as JSON"
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
                data-tip="Stamp as billed — guards against double-billing"
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
