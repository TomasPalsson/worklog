import { useId } from "react";
import { InfoIcon } from "./mirres-art";
import { MirresPopover } from "./MirresPopover";
import { MirresStatusIcon } from "./MirresStatusIcon";
import { stripCustomer } from "@/lib/mirresOverview";
import { rowChip, statusMeta } from "@/lib/mirresStatus";
import type { LineBilling } from "@/lib/tempo_line_contract";

/** The quiet one-line Mirres status under a ticket line, with a details popover. */
export function MirresLine({ billing, issue }: { billing: LineBilling; issue: string }) {
  const id = useId();
  const status = statusMeta(billing);
  const chip = rowChip(billing);
  const project = stripCustomer(billing.project, billing.customer);
  const anchor = `--a${id.replace(/:/g, "")}`;
  const label = `Mirres details for ${issue}`;
  return (
    <span className="mirres-line" data-tone={status.tone} title={status.hint}>
      <MirresStatusIcon kind={status.kind} />
      <span className="mirres-line-customer">{billing.customer ?? "Unknown customer"}</span>
      {chip ? (
        <span className="mirres-line-chip">{chip}</span>
      ) : (
        <span className="mirres-sr">{status.label}</span>
      )}
      {project && <span className="mirres-line-project">· {project}</span>}
      {/* Clicks and keys here must not toggle the parent <details>. */}
      <span
        className="mirres-info"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          popoverTarget={id}
          aria-label={label}
          className="mirres-info-btn"
          style={{ ["anchorName" as string]: anchor }}
        >
          <InfoIcon size={16} />
        </button>
        <MirresPopover id={id} anchor={anchor} label={label} billing={billing} status={status} />
      </span>
    </span>
  );
}
