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
  {
    title: "Mirres",
    keys: ["mirres_gateway_url", "mirres_token_url", "mirres_client_id", "mirres_client_secret"],
  },
  { title: "GitHub", keys: ["github_user", "github_token"] },
  { title: "Slack", keys: ["slack_user_token"] },
  {
    title: "Google Calendar",
    keys: ["google_client_id", "google_client_secret", "google_refresh_token"],
  },
  {
    title: "AI summaries",
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
  mirres_gateway_url: "Gateway URL",
  mirres_token_url: "Token URL",
  mirres_client_id: "Client ID",
  mirres_client_secret: "Client secret",
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
const PROVIDER_LABELS: Record<string, string> = {
  "": "Default (Claude Code)",
  claude_subprocess: "Claude Code",
  litellm: "LiteLLM",
};

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
              {PROVIDER_LABELS[o] ?? o}
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
            ? "Saved — type to replace"
            : field.sensitive
              ? "Not set"
              : ""
        }
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

/** Groups that count toward "connected" — the estimator's keys are
 * alternatives (one provider or another), so it has no all-set state. */
const STATUS_GROUPS = GROUPS.filter((g) => g.title !== "AI summaries");
const STATUS_COPY = { on: "Connected", part: "Partly set up", off: "Not set up" };

function groupFields(secrets: SettingField[], keys: string[]): SettingField[] {
  return keys
    .map((k) => secrets.find((f) => f.key === k))
    .filter((f): f is SettingField => !!f);
}

function statusOf(fields: SettingField[]): keyof typeof STATUS_COPY {
  const set = fields.filter((f) => f.present).length;
  return set === 0 ? "off" : set === fields.length ? "on" : "part";
}

/** How many services have every key stored, for the section index. */
export function connectedCount(secrets: SettingField[]): { done: number; total: number } {
  const groups = STATUS_GROUPS.map((g) => groupFields(secrets, g.keys)).filter((f) => f.length);
  return { done: groups.filter((f) => statusOf(f) === "on").length, total: groups.length };
}

function Group({
  title,
  fields,
  values,
  onChange,
}: {
  title: string;
  fields: SettingField[];
  values: Record<string, string>;
  onChange: (key: string, value: string) => void;
}) {
  const status = STATUS_GROUPS.some((g) => g.title === title) ? statusOf(fields) : null;
  return (
    <fieldset className="set-service">
      <legend>
        {title}
        {status && (
          <span className="set-pill" data-status={status}>
            {STATUS_COPY[status]}
          </span>
        )}
      </legend>
      <div className="settings-grid-2">
        {fields.map((f) => (
          <Field key={f.key} field={f} value={values[f.key] ?? ""} onChange={(v) => onChange(f.key, v)} />
        ))}
      </div>
    </fieldset>
  );
}

/** Every known secret key, grouped by service, plus an "Other" catch-all
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
    <section id="connections" className="set-card" aria-labelledby="connections-title">
      <h2 id="connections-title">Connections</h2>
      <p className="settings-hint">
        The keys worklog uses to read Jira, Tempo, GitHub, Slack and your calendar. A saved
        key is never shown again; leave its field empty to keep it.
      </p>
      {GROUPS.map((g) => {
        const fields = groupFields(secrets, g.keys);
        if (fields.length === 0) return null;
        return <Group key={g.title} title={g.title} fields={fields} values={values} onChange={onChange} />;
      })}
      {otherKeys.length > 0 && (
        <Group title="Other" fields={otherKeys} values={values} onChange={onChange} />
      )}
    </section>
  );
}
