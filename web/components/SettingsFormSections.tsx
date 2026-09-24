"use client";

// The settings-dialog body: classification, timezone, pruner and routing
// sections, plus the grouped credential fields. Split out of
// SettingsPanel.tsx to keep that component under the size gate — every
// piece here is a plain controlled view over the form state the parent
// owns.

import type { SettingsView } from "@/lib/types";
import type { SettingsFormState } from "@/lib/settingsForm";
import { CredentialGroups } from "./CredentialFields";
import { RoutingFields, RoutingStatusAndRules } from "./RoutingSettings";

function ClassificationSection({
  form,
  patch,
  configPath,
}: {
  form: SettingsFormState;
  patch: (p: Partial<SettingsFormState>) => void;
  configPath: string | null;
}) {
  return (
    <section className="settings-section">
      <h3>Classification</h3>
      <p className="settings-hint">
        One path glob per line. <code>~</code> expands to home; suffix{" "}
        <code>{"/**"}</code> matches any depth. Work patterns win over
        personal; anything unmatched defaults to{" "}
        <code>{"~/Desktop/Work/**"}</code> = work, else personal.
      </p>
      <div className="settings-grid-2">
        <label className="settings-field">
          <span>Work paths</span>
          <textarea
            rows={4}
            value={form.work}
            spellCheck={false}
            placeholder="~/Desktop/Work/**"
            onChange={(e) => patch({ work: e.target.value })}
          />
        </label>
        <label className="settings-field">
          <span>Personal paths</span>
          <textarea
            rows={4}
            value={form.personal}
            spellCheck={false}
            placeholder="~/Desktop/Projects/**"
            onChange={(e) => patch({ personal: e.target.value })}
          />
        </label>
      </div>
      {configPath && <p className="settings-path">{configPath}</p>}
    </section>
  );
}

function TimezoneSection({
  form,
  patch,
}: {
  form: SettingsFormState;
  patch: (p: Partial<SettingsFormState>) => void;
}) {
  return (
    <section className="settings-section">
      <h3>Timezone</h3>
      <p className="settings-hint">
        Fixed offset for day bucketing — e.g. <code>+01:00</code>,{" "}
        <code>-05:00</code>, or <code>UTC</code>. Named zones aren&rsquo;t
        supported.
      </p>
      <label className="settings-field settings-field-narrow">
        <span>WORKLOG_TZ</span>
        <input
          type="text"
          value={form.tz}
          placeholder="UTC"
          autoComplete="off"
          onChange={(e) => patch({ tz: e.target.value })}
        />
      </label>
    </section>
  );
}

function PrunerSection({
  form,
  patch,
}: {
  form: SettingsFormState;
  patch: (p: Partial<SettingsFormState>) => void;
}) {
  return (
    <section className="settings-section">
      <h3>Billing cycle pruner</h3>
      <p className="settings-hint">
        Automatically deletes work data once its billing cycle has closed.
        Cycles run from the start day through the day before the next
        month&rsquo;s start day; the close day is the last day hours can
        still be added to the cycle that just ended.
      </p>
      <label className="settings-field">
        <span>Enable automatic pruning</span>
        <input
          type="checkbox"
          checked={form.pruneEnabled}
          onChange={(e) => patch({ pruneEnabled: e.target.checked })}
        />
      </label>
      <div className="settings-grid-2">
        <label className="settings-field settings-field-narrow">
          <span>Cycle start day</span>
          <input
            type="number"
            min={1}
            max={31}
            value={form.cycleStartDay}
            placeholder="20"
            autoComplete="off"
            onChange={(e) => patch({ cycleStartDay: e.target.value })}
          />
        </label>
        <label className="settings-field settings-field-narrow">
          <span>Close day</span>
          <input
            type="number"
            min={1}
            max={31}
            value={form.closeDay}
            placeholder="23"
            autoComplete="off"
            onChange={(e) => patch({ closeDay: e.target.value })}
          />
        </label>
      </div>
    </section>
  );
}

function RoutingSection({
  form,
  patch,
  day,
}: {
  form: SettingsFormState;
  patch: (p: Partial<SettingsFormState>) => void;
  day: string;
}) {
  return (
    <section className="settings-section">
      <h3>Browser + Slack routing</h3>
      <p className="settings-hint">
        Heartbeats and Slack messages are labelled by the first match of a
        hard rule, then a model guess at or above the threshold, else left
        unsorted for the day&rsquo;s Unsorted list.
      </p>
      <RoutingFields
        workHours={form.workHours}
        onWorkHoursChange={(v) => patch({ workHours: v })}
        abstainMargin={form.abstainMargin}
        onAbstainMarginChange={(v) => patch({ abstainMargin: v })}
        runnerUpRatio={form.runnerUpRatio}
        onRunnerUpRatioChange={(v) => patch({ runnerUpRatio: v })}
      />
      <RoutingStatusAndRules day={day} />
    </section>
  );
}

export function SettingsBody({
  view,
  form,
  setForm,
  day,
}: {
  view: SettingsView;
  form: SettingsFormState;
  setForm: (updater: (f: SettingsFormState) => SettingsFormState) => void;
  day: string;
}) {
  const patch = (p: Partial<SettingsFormState>) => setForm((f) => ({ ...f, ...p }));

  return (
    <div className="settings-body">
      <ClassificationSection
        form={form}
        patch={patch}
        configPath={view.personal_config_path}
      />
      <TimezoneSection form={form} patch={patch} />
      <PrunerSection form={form} patch={patch} />
      <RoutingSection form={form} patch={patch} day={day} />
      <CredentialGroups
        secrets={view.secrets}
        values={form.secretInputs}
        onChange={(key, value) =>
          patch({ secretInputs: { ...form.secretInputs, [key]: value } })
        }
      />
    </div>
  );
}
