"use client";

// The /settings page body: one card per topic, plus the section index
// beside it. Every piece is a plain controlled view over the form state
// SettingsPanel owns.

import type { ReactNode } from "react";
import type { SettingsView } from "@/lib/types";
import type { SettingsFormState } from "@/lib/settingsForm";
import { CredentialGroups, connectedCount } from "./CredentialFields";
import { RoutingFields, RoutingStatusAndRules } from "./RoutingSettings";
import { VerdictControl } from "./VerdictControl";

type Patch = (p: Partial<SettingsFormState>) => void;

const SECTIONS = [
  ["verdict", "Verdict"],
  ["folders", "Work or personal"],
  ["time", "Time zone"],
  ["cleanup", "Old data cleanup"],
  ["sorting", "Browser & Slack"],
  ["connections", "Connections"],
] as const;

export function SettingsIndex({ view }: { view: SettingsView }) {
  const { done, total } = connectedCount(view.secrets);
  return (
    <nav className="set-index" aria-label="Settings sections">
      <ol>
        {SECTIONS.map(([id, label]) => (
          <li key={id}>
            <a href={`#${id}`}>
              {label}
              {id === "connections" && total > 0 && (
                <span className="set-index-count">
                  {done} of {total}
                </span>
              )}
            </a>
          </li>
        ))}
      </ol>
    </nav>
  );
}

function Card({
  id,
  title,
  lede,
  children,
}: {
  id: string;
  title: string;
  lede?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section id={id} className="set-card" aria-labelledby={`${id}-title`}>
      <h2 id={`${id}-title`}>{title}</h2>
      {lede && <p className="settings-hint">{lede}</p>}
      {children}
    </section>
  );
}

function FoldersCard({ form, patch, configPath }: { form: SettingsFormState; patch: Patch; configPath: string | null }) {
  return (
    <Card
      id="folders"
      title="Work or personal"
      lede={
        <>
          Which folders count as work. One folder pattern per line; <code>~</code> is your
          home folder and <code>{"/**"}</code> means &ldquo;and everything inside&rdquo;. If
          a folder matches both lists, work wins. Anything not listed counts as work
          when it is under <code>{"~/Desktop/Work"}</code>, otherwise personal.
        </>
      }
    >
      <div className="settings-grid-2">
        <label className="settings-field">
          <span>Work folders</span>
          <textarea
            rows={5}
            value={form.work}
            spellCheck={false}
            placeholder="~/Desktop/Work/**"
            onChange={(e) => patch({ work: e.target.value })}
          />
        </label>
        <label className="settings-field">
          <span>Personal folders</span>
          <textarea
            rows={5}
            value={form.personal}
            spellCheck={false}
            placeholder="~/Desktop/Projects/**"
            onChange={(e) => patch({ personal: e.target.value })}
          />
        </label>
      </div>
      {configPath && <p className="settings-path">Stored in {configPath}</p>}
    </Card>
  );
}

function TimeCard({ form, patch }: { form: SettingsFormState; patch: Patch }) {
  return (
    <Card
      id="time"
      title="Time zone"
      lede={
        <>
          Decides where one day ends and the next begins. Use an offset from UTC, like{" "}
          <code>+01:00</code> or <code>-05:00</code>, or just <code>UTC</code>. City names
          don&rsquo;t work, so change it by hand when summer time starts or ends.
        </>
      }
    >
      <label className="settings-field settings-field-narrow">
        <span>Offset from UTC</span>
        <input
          type="text"
          value={form.tz}
          placeholder="UTC"
          autoComplete="off"
          spellCheck={false}
          pattern="UTC|[+\-]\d{2}:\d{2}"
          onChange={(e) => patch({ tz: e.target.value })}
        />
        <small className="set-invalid">Use UTC or an offset like +01:00.</small>
      </label>
    </Card>
  );
}

function CleanupCard({ form, patch }: { form: SettingsFormState; patch: Patch }) {
  return (
    <Card
      id="cleanup"
      title="Old data cleanup"
      lede="Deletes work data on its own once a billing month has closed. A billing month runs from the start day to the day before the next month's start day. The close day is the last day you can still add hours to the month that just ended."
    >
      <label className="settings-field set-check">
        <input
          type="checkbox"
          checked={form.pruneEnabled}
          onChange={(e) => patch({ pruneEnabled: e.target.checked })}
        />
        <span>Enable automatic pruning</span>
      </label>
      <div className="settings-grid-2" data-off={!form.pruneEnabled || undefined}>
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
          <small>Day of the month, 1–31</small>
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
          <small>Day of the month, 1–31</small>
        </label>
      </div>
    </Card>
  );
}

function SortingCard({ form, patch, day }: { form: SettingsFormState; patch: Patch; day: string }) {
  return (
    <Card
      id="sorting"
      title="Browser & Slack"
      lede="Browser tabs and Slack messages go to a project by your own rules first, then by a model guess when it is sure enough. Anything else waits in the day's Unsorted list."
    >
      <RoutingFields
        workHours={form.workHours}
        onWorkHoursChange={(v) => patch({ workHours: v })}
        abstainMargin={form.abstainMargin}
        onAbstainMarginChange={(v) => patch({ abstainMargin: v })}
        runnerUpRatio={form.runnerUpRatio}
        onRunnerUpRatioChange={(v) => patch({ runnerUpRatio: v })}
      />
      <RoutingStatusAndRules day={day} />
    </Card>
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
  const patch: Patch = (p) => setForm((f) => ({ ...f, ...p }));

  return (
    <div className="set-cards">
      <VerdictControl day={day} autoSend={form.autoSend} onAutoSend={(autoSend) => patch({ autoSend })} />
      <FoldersCard form={form} patch={patch} configPath={view.personal_config_path} />
      <TimeCard form={form} patch={patch} />
      <CleanupCard form={form} patch={patch} />
      <SortingCard form={form} patch={patch} day={day} />
      <CredentialGroups
        secrets={view.secrets}
        values={form.secretInputs}
        onChange={(key, value) => patch({ secretInputs: { ...form.secretInputs, [key]: value } })}
      />
    </div>
  );
}
