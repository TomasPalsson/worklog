"use client";

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Loader2, Settings, X } from "lucide-react";
import type { SettingsView } from "@/lib/types";
import { fetchSettings, saveSettings } from "@/app/actions";
import { toast } from "@/lib/toast";
import {
  buildSettingsUpdate,
  formStateFromView,
  type SettingsFormState,
} from "@/lib/settingsForm";
import { SettingsBody } from "./SettingsFormSections";

interface Props {
  /** The day page the panel was opened from — saving revalidates it so a
   * classification change is reflected without a manual reload. */
  day: string;
}

const EMPTY_FORM: SettingsFormState = {
  work: "",
  personal: "",
  tz: "",
  pruneEnabled: true,
  cycleStartDay: "",
  closeDay: "",
  workHours: "",
  routeThreshold: "",
  secretInputs: {},
};

export function SettingsPanel({ day }: Props) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<SettingsView | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState<SettingsFormState>(EMPTY_FORM);

  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);

  const hydrate = useCallback((v: SettingsView) => {
    setView(v);
    setForm(formStateFromView(v));
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    const r = await fetchSettings();
    setLoading(false);
    if (!r.ok) {
      toast.error(`Couldn't load settings — ${r.error}`);
      setOpen(false);
      return;
    }
    hydrate(r.data);
  }, [hydrate]);

  // Load fresh settings each time the panel opens — cheap, and avoids
  // showing stale credential-present state after an external change.
  useEffect(() => {
    if (open) load();
  }, [open, load]);

  // Escape closes. Bound only while open.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !saving) setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, saving]);

  async function onSave() {
    if (!view) return;
    const update = buildSettingsUpdate(view, form);
    if (!update) {
      toast.ok("No changes to save.");
      return;
    }
    setSaving(true);
    const r = await saveSettings(update, day);
    setSaving(false);
    if (!r.ok) {
      toast.error(`Save failed — ${r.error}`);
      return;
    }
    const rc = r.data.reclassified;
    if (rc && rc.changed_to_personal + rc.changed_to_work > 0) {
      toast.ok(
        `Saved. Reclassified ${rc.changed_to_personal} → personal, ` +
          `${rc.changed_to_work} → work.`,
      );
    } else {
      toast.ok("Settings saved.");
    }
    // Re-hydrate from the authoritative response so the next diff is
    // computed against what's actually stored now.
    hydrate(r.data);
    setOpen(false);
  }

  return (
    <>
      <button
        type="button"
        className="theme-toggle"
        onClick={() => setOpen(true)}
        aria-label="Open settings"
        data-tip="Settings"
      >
        <Settings size={15} strokeWidth={1.75} />
      </button>

      {open && (
        <div
          className="settings-overlay"
          onMouseDown={(e) => {
            // Close only when the backdrop itself is clicked, not the panel.
            if (e.target === e.currentTarget && !saving) setOpen(false);
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
              <h2 id={titleId}>Settings</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="Close settings"
                disabled={saving}
                onClick={() => setOpen(false)}
              >
                <X size={16} />
              </button>
            </header>

            {loading || !view ? (
              <div className="settings-loading">
                <Loader2 className="spin" size={20} />
                <span>Loading…</span>
              </div>
            ) : (
              <SettingsBody view={view} form={form} setForm={setForm} day={day} />
            )}

            <footer className="settings-footer">
              <button
                type="button"
                className="action-btn"
                disabled={saving || loading}
                onClick={() => setOpen(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="action-btn primary"
                disabled={saving || loading || !view}
                onClick={onSave}
              >
                {saving ? <Loader2 className="spin" size={15} /> : null}
                {saving ? "Saving…" : "Save changes"}
              </button>
            </footer>
          </div>
        </div>
      )}
    </>
  );
}
