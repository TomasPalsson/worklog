"use client";

import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import type { SettingsUpdate, SettingsView } from "@/lib/types";
import { fetchSettings, saveSettings } from "@/app/actions";
import { toast } from "@/lib/toast";
import {
  buildSettingsUpdate,
  formStateFromView,
  type SettingsFormState,
} from "@/lib/settingsForm";
import { connectedCount } from "./CredentialFields";
import { SettingsBody } from "./SettingsFormSections";
import { SettingsIndex } from "./SettingsNav";

interface Props {
  /** The day the user came from — saving revalidates it so a
   * classification change is reflected without a manual reload. */
  day: string;
}

/** The /settings page body: loads once, edits in place, and saves every
 * changed field together from the sticky bar. Verdict on/off is the one
 * control that applies at once (it starts a process). */
export function SettingsPanel({ day }: Props) {
  const [view, setView] = useState<SettingsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState<SettingsFormState | null>(null);

  const hydrate = useCallback((v: SettingsView) => {
    setView(v);
    setForm(formStateFromView(v));
  }, []);

  const load = useCallback(async () => {
    setError(null);
    const r = await fetchSettings();
    if (r.ok) hydrate(r.data);
    else setError(r.error);
  }, [hydrate]);

  useEffect(() => {
    void load();
  }, [load]);

  const update = view && form ? buildSettingsUpdate(view, form) : null;
  const dirty = update !== null;

  // Leaving with unsaved edits asks first.
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  // ⌘S / Ctrl+S saves from anywhere on the page.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (dirty && !saving) void onSave();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  async function onSave() {
    if (!update) return;
    setSaving(true);
    const r = await saveSettings(update, day);
    setSaving(false);
    if (!r.ok) {
      toast.error(`Couldn't save — ${r.error}. Your edits are still here; try again.`);
      return;
    }
    const rc = r.data.reclassified;
    if (rc && rc.changed_to_personal + rc.changed_to_work > 0) {
      toast.ok(
        `Saved. Moved ${rc.changed_to_personal} block(s) to personal and ` +
          `${rc.changed_to_work} to work.`,
      );
    } else {
      toast.ok("Settings saved.");
    }
    // Re-hydrate from the stored values so the next diff is against them.
    hydrate(r.data);
  }

  if (error) {
    return (
      <div className="set-state" role="alert">
        <p>
          <strong>Couldn&rsquo;t load your settings.</strong> worklog said: {error}
        </p>
        <p>Check that the worklog daemon is running, then try again.</p>
        <button type="button" className="action-btn" onClick={() => void load()}>
          Try again
        </button>
      </div>
    );
  }

  if (!view || !form) {
    return (
      <div className="set-state" role="status">
        <Loader2 className="spin" size={18} />
        <span>Loading your settings…</span>
      </div>
    );
  }

  return (
    <div className="set-layout">
      <SettingsIndex connections={connectedCount(view.secrets, view.gcal)} />
      <div className="set-main">
        <SettingsBody
          view={view}
          form={form}
          setForm={(fn) => setForm((f) => (f ? fn(f) : f))}
          day={day}
        />
        <SaveBar
          changes={update ? changeCount(update) : 0}
          saving={saving}
          onDiscard={() => hydrate(view)}
          onSave={onSave}
        />
      </div>
    </div>
  );
}

/** Each changed setting counts once; each changed key counts once. */
function changeCount(update: SettingsUpdate): number {
  const { secrets, ...rest } = update;
  return Object.keys(rest).length + Object.keys(secrets ?? {}).length;
}

function SaveBar({
  changes,
  saving,
  onDiscard,
  onSave,
}: {
  changes: number;
  saving: boolean;
  onDiscard: () => void;
  onSave: () => void;
}) {
  const dirty = changes > 0;
  return (
    <div className="set-savebar" data-dirty={dirty || undefined}>
      <span className="set-savebar-note" role="status">
        {dirty ? `${changes} unsaved ${changes === 1 ? "change" : "changes"}` : "All changes saved."}
      </span>
      <button type="button" className="action-btn" disabled={!dirty || saving} onClick={onDiscard}>
        Discard changes
      </button>
      <button type="button" className="action-btn primary" disabled={!dirty || saving} onClick={onSave}>
        {saving && <Loader2 className="spin" size={15} />}
        {saving ? "Saving…" : "Save changes"}
        {!saving && <kbd className="set-kbd" aria-hidden="true">⌘S</kbd>}
      </button>
    </div>
  );
}
