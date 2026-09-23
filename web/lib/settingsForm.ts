// Pure form-state helpers for SettingsPanel — split out so the component
// itself stays under the size gate. No React here; hydrate/diff logic
// only, so it's trivially testable in isolation if that's ever needed.

import type { SettingsUpdate, SettingsView } from "./types";

export interface SettingsFormState {
  work: string;
  personal: string;
  tz: string;
  pruneEnabled: boolean;
  cycleStartDay: string;
  closeDay: string;
  workHours: string;
  routeThreshold: string;
  secretInputs: Record<string, string>;
}

function splitLines(s: string): string[] {
  return s
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

/** The value a secret field's input starts at — empty for masked tokens
 * (we never receive them), the stored value otherwise. The save diff is
 * computed against exactly this. */
export function initialSecretInput(sensitive: boolean, value: string | null): string {
  return sensitive ? "" : (value ?? "");
}

/** Hydrate editable form state from a freshly (re)loaded settings snapshot. */
export function formStateFromView(v: SettingsView): SettingsFormState {
  const secretInputs: Record<string, string> = {};
  for (const f of v.secrets) secretInputs[f.key] = initialSecretInput(f.sensitive, f.value);
  return {
    work: v.personal.work.join("\n"),
    personal: v.personal.personal.join("\n"),
    tz: v.timezone,
    pruneEnabled: v.prune_enabled,
    cycleStartDay: String(v.cycle_start_day),
    closeDay: String(v.close_day),
    workHours: v.work_hours,
    routeThreshold: String(v.route_threshold),
    secretInputs,
  };
}

/** Diff the live form against the last-loaded view. `null` when nothing changed. */
export function buildSettingsUpdate(
  view: SettingsView,
  form: SettingsFormState,
): SettingsUpdate | null {
  const update: SettingsUpdate = {};

  const workArr = splitLines(form.work);
  const personalArr = splitLines(form.personal);
  if (
    workArr.join("\n") !== view.personal.work.join("\n") ||
    personalArr.join("\n") !== view.personal.personal.join("\n")
  ) {
    update.personal = { work: workArr, personal: personalArr };
  }

  if (form.tz.trim() !== view.timezone.trim()) update.timezone = form.tz.trim();
  if (form.pruneEnabled !== view.prune_enabled) update.prune_enabled = form.pruneEnabled;
  if (form.cycleStartDay.trim() !== String(view.cycle_start_day)) {
    update.cycle_start_day = Number(form.cycleStartDay.trim());
  }
  if (form.closeDay.trim() !== String(view.close_day)) {
    update.close_day = Number(form.closeDay.trim());
  }
  if (form.workHours.trim() !== view.work_hours.trim()) {
    update.work_hours = form.workHours.trim();
  }
  const thresholdNum = Number(form.routeThreshold.trim());
  if (!Number.isNaN(thresholdNum) && thresholdNum !== view.route_threshold) {
    update.route_threshold = thresholdNum;
  }

  const secrets: Record<string, string> = {};
  for (const f of view.secrets) {
    const initial = initialSecretInput(f.sensitive, f.value);
    const cur = form.secretInputs[f.key] ?? initial;
    if (cur !== initial) secrets[f.key] = cur;
  }
  if (Object.keys(secrets).length > 0) update.secrets = secrets;

  const nothingChanged =
    !update.personal &&
    update.timezone === undefined &&
    !update.secrets &&
    update.prune_enabled === undefined &&
    update.cycle_start_day === undefined &&
    update.close_day === undefined &&
    update.work_hours === undefined &&
    update.route_threshold === undefined;

  return nothingChanged ? null : update;
}
