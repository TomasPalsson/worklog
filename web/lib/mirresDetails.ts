// Pure text for the "who is responsible, contract hours" line shown under a
// ticket line's Mirres status (Day page) and a project (Mirres page).

import type { MirresDetails } from "./tempo_line_contract";

export type DetailPart =
  | { kind: "person"; label: string; name: string; email: string | null }
  | { kind: "text"; text: string }
  | { kind: "link"; text: string; href: string };

const PERIOD_TEXT: Record<string, string> = {
  MONTHLY: "this month",
  YEARLY: "this year",
  ONE_OFF: "in total",
};

/** Icelandic decimal comma, trailing ",0" dropped: 1.5 -> "1,5", 10 -> "10". */
const hours = (h: number) => h.toFixed(1).replace(/\.0$/, "").replace(".", ",");

export function billingDetailsText(details: MirresDetails | null | undefined): DetailPart[] {
  if (!details) return [];
  const parts: DetailPart[] = [];
  const { owner, responsible } = details;
  if (owner) parts.push({ kind: "person", label: "Owner", name: owner.name, email: owner.email });
  if (responsible && responsible.name !== owner?.name) {
    parts.push({ kind: "person", label: "Customer lead", name: responsible.name, email: responsible.email });
  }
  if (details.allowance_hours != null) {
    const when = PERIOD_TEXT[details.period ?? ""] ?? "";
    const a = hours(details.allowance_hours);
    const text =
      details.remaining_hours != null
        ? `${hours(details.remaining_hours)} of ${a} h left${when && ` ${when}`}`
        : `${a} h included${when && ` ${when}`}`;
    parts.push({ kind: "text", text });
  }
  if (details.due_date) parts.push({ kind: "text", text: `Due ${details.due_date}` });
  if (details.contract_url) parts.push({ kind: "link", text: "Contract", href: details.contract_url });
  return parts;
}
