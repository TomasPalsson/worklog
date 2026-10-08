// One status per Mirres project: drives the icon, label and tone everywhere
// (day-page row, popover, /mirres page).

import type { LineBilling } from "./tempo_line_contract";

export type StatusKind = "billable" | "included" | "fixed" | "internal" | "missing" | "not_billable";
export type StatusTone = "sage" | "muted" | "amber";

export const STATUS_META: Record<StatusKind, { label: string; hint: string; tone: StatusTone }> = {
  billable: { label: "billable", hint: "Billed to the customer", tone: "sage" },
  included: { label: "covered by contract", hint: "Covered by the contract's included hours; counts toward the 70% goal", tone: "sage" },
  fixed: { label: "fixed price", hint: "Not billed by the hour", tone: "muted" },
  internal: { label: "internal", hint: "Internal work, not billed", tone: "muted" },
  missing: { label: "contract missing", hint: "Mirres has no usable contract for this project; counted as billable until it's fixed", tone: "amber" },
  not_billable: { label: "not billable", hint: "Not billed", tone: "muted" },
};

const MISSING = ["Samning vantar", "Tímafjölda vantar", "Fleiri en ein", "Óþekkt", "Ekki virkt"];

type StatusInput = Pick<LineBilling, "class" | "project_type" | "warning">;

export function statusKind(b: StatusInput): StatusKind {
  const w = b.warning;
  if (w && MISSING.some((m) => w.startsWith(m))) return "missing";
  if (b.class === "billable") return "billable";
  if (b.class === "included") return "included";
  const t = b.project_type ?? "";
  if (t.includes("Fast verð") || t.includes("Innifalið")) return "fixed";
  if (t.includes("(Apró)")) return "internal";
  return "not_billable";
}

/** Status meta for a row; a subscription ("Innifalið") is a fixed kind labelled as such. */
export function statusMeta(b: StatusInput) {
  const kind = statusKind(b);
  const meta = STATUS_META[kind];
  const subscription = kind === "fixed" && !(b.project_type ?? "").includes("Fast verð");
  return { kind, ...meta, label: subscription ? "subscription" : meta.label };
}
