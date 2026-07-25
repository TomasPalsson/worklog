"use client";

import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Loader2, Settings, X } from "lucide-react";
import type { SettingField, SettingsUpdate, SettingsView } from "@/lib/types";
import { fetchSettings, saveSettings } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  /** The day page the panel was opened from — saving revalidates it so a
   * classification change is reflected without a manual reload. */
  day: string;
}

/** Logical grouping + human labels for the flat KNOWN_KEYS list the
 * daemon returns. Keys not listed here still render under "Other" so a
 * newly-added daemon key is never silently dropped. */
const GROUPS: { title: string; keys: string[] }[] = [
  {
    title: "Jira / Tempo",
    keys: [
      "jira_base_url",
      "jira_email",
      "jira_account_id",
      "jira_account_field_id",
      "jira_api_token",
      "tempo_api_token",
    ],
  },
  { title: "GitHub", keys: ["github_user", "github_token"] },
  {
    title: "Google Calendar",
    keys: ["google_client_id", "google_client_secret", "google_refresh_token"],
  },
  {
    title: "Estimator",
    keys: [
      "worklog_estimator_provider",
      "anthropic_api_key",
      "litellm_base_url",
      "litellm_api_key",
      "litellm_model",
    ],
  },
];

const LABELS: Record<string, string> = {
  jira_base_url: "Base URL",
  jira_email: "Email",
  jira_account_id: "Account ID",
  jira_account_field_id: "Account field (customfield_…)",
  jira_api_token: "API token",
  tempo_api_token: "Tempo API token",
  github_user: "Username",
  github_token: "Token",
  google_client_id: "Client ID",
  google_client_secret: "Client secret",
  google_refresh_token: "Refresh token",
  worklog_estimator_provider: "Provider",
  anthropic_api_key: "Anthropic API key",
  litellm_base_url: "LiteLLM base URL",
  litellm_api_key: "LiteLLM API key",
  litellm_model: "LiteLLM model",
};

const PROVIDER_OPTIONS = ["", "claude_subprocess", "litellm"];

function splitLines(s: string): string[] {
  return s
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

/** The value a field's input starts at — empty for masked tokens (we
 * never receive them), the stored value otherwise. The save diff is
 * computed against exactly this. */
function initialInput(f: SettingField): string {
  return f.sensitive ? "" : (f.value ?? "");
}

export function SettingsPanel({ day }: Props) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<SettingsView | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  // Editable form state, hydrated from `view` each time the panel loads.
  const [work, setWork] = useState("");
  const [personal, setPersonal] = useState("");
  const [tz, setTz] = useState("");
  const [secretInputs, setSecretInputs] = useState<Record<string, string>>({});

  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);

  const hydrate = useCallback((v: SettingsView) => {
    setView(v);
    setWork(v.personal.work.join("\n"));
    setPersonal(v.personal.personal.join("\n"));
    setTz(v.timezone);
    const inputs: Record<string, string> = {};
    for (const f of v.secrets) inputs[f.key] = initialInput(f);
    setSecretInputs(inputs);
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

  function buildUpdate(): SettingsUpdate | null {
    if (!view) return null;
    const update: SettingsUpdate = {};

    const workArr = splitLines(work);
    const personalArr = splitLines(personal);
    const patternsChanged =
      workArr.join("\n") !== view.personal.work.join("\n") ||
      personalArr.join("\n") !== view.personal.personal.join("\n");
    if (patternsChanged) {
      update.personal = { work: workArr, personal: personalArr };
    }

    if (tz.trim() !== view.timezone.trim()) update.timezone = tz.trim();

    const secrets: Record<string, string> = {};
    for (const f of view.secrets) {
      const initial = initialInput(f);
      const cur = secretInputs[f.key] ?? initial;
      if (cur !== initial) secrets[f.key] = cur;
    }
    if (Object.keys(secrets).length > 0) update.secrets = secrets;

    if (
      !update.personal &&
      update.timezone === undefined &&
      !update.secrets
    ) {
      return null;
    }
    return update;
  }

  async function onSave() {
    const update = buildUpdate();
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

  function renderField(f: SettingField) {
    const label = LABELS[f.key] ?? f.key;
    const value = secretInputs[f.key] ?? "";
    const onChange = (v: string) =>
      setSecretInputs((prev) => ({ ...prev, [f.key]: v }));

    if (f.key === "worklog_estimator_provider") {
      return (
        <label key={f.key} className="settings-field">
          <span>{label}</span>
          <select value={value} onChange={(e) => onChange(e.target.value)}>
            {PROVIDER_OPTIONS.map((o) => (
              <option key={o} value={o}>
                {o === "" ? "default (claude -p)" : o}
              </option>
            ))}
          </select>
        </label>
      );
    }

    return (
      <label key={f.key} className="settings-field">
        <span>
          {label}
          {f.sensitive && f.present && (
            <em className="settings-stored" title="A value is stored">
              {" "}
              · saved
            </em>
          )}
        </span>
        <input
          type={f.sensitive ? "password" : "text"}
          value={value}
          autoComplete="off"
          placeholder={
            f.sensitive && f.present
              ? "•••••••• (leave blank to keep)"
              : f.sensitive
                ? "not set"
                : ""
          }
          onChange={(e) => onChange(e.target.value)}
        />
      </label>
    );
  }

  // Keys the daemon returned but our static GROUPS don't mention — render
  // them so nothing is hidden.
  const knownInGroups = new Set(GROUPS.flatMap((g) => g.keys));
  const otherKeys = (view?.secrets ?? []).filter(
    (f) => !knownInGroups.has(f.key),
  );

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
              <div className="settings-body">
                <section className="settings-section">
                  <h3>Classification</h3>
                  <p className="settings-hint">
                    One path glob per line. <code>~</code> expands to home;
                    suffix <code>{"/**"}</code> matches any depth. Work
                    patterns win over personal; anything unmatched defaults
                    to <code>{"~/Desktop/Work/**"}</code> = work, else
                    personal.
                  </p>
                  <div className="settings-grid-2">
                    <label className="settings-field">
                      <span>Work paths</span>
                      <textarea
                        rows={4}
                        value={work}
                        spellCheck={false}
                        placeholder="~/Desktop/Work/**"
                        onChange={(e) => setWork(e.target.value)}
                      />
                    </label>
                    <label className="settings-field">
                      <span>Personal paths</span>
                      <textarea
                        rows={4}
                        value={personal}
                        spellCheck={false}
                        placeholder="~/Desktop/Projects/**"
                        onChange={(e) => setPersonal(e.target.value)}
                      />
                    </label>
                  </div>
                  {view.personal_config_path && (
                    <p className="settings-path">{view.personal_config_path}</p>
                  )}
                </section>

                <section className="settings-section">
                  <h3>Timezone</h3>
                  <p className="settings-hint">
                    Fixed offset for day bucketing — e.g. <code>+01:00</code>,{" "}
                    <code>-05:00</code>, or <code>UTC</code>. Named zones
                    aren&rsquo;t supported.
                  </p>
                  <label className="settings-field settings-field-narrow">
                    <span>WORKLOG_TZ</span>
                    <input
                      type="text"
                      value={tz}
                      placeholder="UTC"
                      autoComplete="off"
                      onChange={(e) => setTz(e.target.value)}
                    />
                  </label>
                </section>

                {GROUPS.map((g) => {
                  const fields = g.keys
                    .map((k) => view.secrets.find((f) => f.key === k))
                    .filter((f): f is SettingField => !!f);
                  if (fields.length === 0) return null;
                  return (
                    <section key={g.title} className="settings-section">
                      <h3>{g.title}</h3>
                      <div className="settings-grid-2">
                        {fields.map(renderField)}
                      </div>
                    </section>
                  );
                })}

                {otherKeys.length > 0 && (
                  <section className="settings-section">
                    <h3>Other</h3>
                    <div className="settings-grid-2">
                      {otherKeys.map(renderField)}
                    </div>
                  </section>
                )}
              </div>
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
