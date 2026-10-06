import { Fragment } from "react";
import { billingDetailsText } from "@/lib/mirresDetails";
import type { MirresDetails } from "@/lib/tempo_line_contract";

/** Owner / customer lead / contract hours line; renders nothing when empty. */
export function BillingDetails({
  details,
  className = "billing-details",
}: {
  details: MirresDetails | null | undefined;
  className?: string;
}) {
  const parts = billingDetailsText(details);
  if (parts.length === 0) return null;
  return (
    <span className={className}>
      {parts.map((p, i) => (
        <Fragment key={i}>
          {i > 0 && " · "}
          {p.kind === "person" ? (
            <>
              {p.label}:{" "}
              {p.email ? <a href={`mailto:${p.email}`}>{p.name}</a> : p.name}
            </>
          ) : p.kind === "link" ? (
            <a href={p.href} target="_blank" rel="noreferrer">
              {p.text}
            </a>
          ) : (
            p.text
          )}
        </Fragment>
      ))}
    </span>
  );
}
