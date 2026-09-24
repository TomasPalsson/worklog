"use client";

// Grouped rendering for the flat KNOWN_KEYS secret list `GET /settings`
// returns. Split out of SettingsPanel.tsx to keep that file under the
// size gate — purely presentational; `secretInputs` state stays with the
// parent settings form.

import type { SettingField } from "@/lib/types";

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
  { title: "Slack", keys: ["slack_user_token"] },
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
  slack_user_token: "User token",
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

function Field({
  field,
  value,
  onChange,
}: {
  field: SettingField;
  value: string;
  onChange: (v: string) => void;
}) {
  const label = LABELS[field.key] ?? field.key;

  if (field.key === "worklog_estimator_provider") {
    return (
      <label className="settings-field">
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
    <label className="settings-field">
      <span>
        {label}
        {field.sensitive && field.present && (
          <em className="settings-stored" title="A value is stored">
            {" "}
            · saved
          </em>
        )}
      </span>
      <input
        type={field.sensitive ? "password" : "text"}
        value={value}
        autoComplete="off"
        placeholder={
          field.sensitive && field.present
            ? "•••••••• (leave blank to keep)"
            : field.sensitive
              ? "not set"
              : ""
        }
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

/** Every known secret key, grouped by provider, plus an "Other" catch-all
 * for daemon keys our static GROUPS don't mention yet — nothing is ever
 * silently hidden. */
export function CredentialGroups({
  secrets,
  values,
  onChange,
}: {
  secrets: SettingField[];
  values: Record<string, string>;
  onChange: (key: string, value: string) => void;
}) {
  const knownInGroups = new Set(GROUPS.flatMap((g) => g.keys));
  const otherKeys = secrets.filter((f) => !knownInGroups.has(f.key));

  return (
    <>
      {GROUPS.map((g) => {
        const fields = g.keys
          .map((k) => secrets.find((f) => f.key === k))
          .filter((f): f is SettingField => !!f);
        if (fields.length === 0) return null;
        return (
          <section key={g.title} className="settings-section">
            <h3>{g.title}</h3>
            <div className="settings-grid-2">
              {fields.map((f) => (
                <Field
                  key={f.key}
                  field={f}
                  value={values[f.key] ?? ""}
                  onChange={(v) => onChange(f.key, v)}
                />
              ))}
            </div>
          </section>
        );
      })}

      {otherKeys.length > 0 && (
        <section className="settings-section">
          <h3>Other</h3>
          <div className="settings-grid-2">
            {otherKeys.map((f) => (
              <Field
                key={f.key}
                field={f}
                value={values[f.key] ?? ""}
                onChange={(v) => onChange(f.key, v)}
              />
            ))}
          </div>
        </section>
      )}
    </>
  );
}
