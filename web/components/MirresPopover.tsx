import { BillingDetails } from "./BillingDetails";
import { ContractMissingIcon } from "./mirres-art";
import { MirresStatusIcon } from "./MirresStatusIcon";
import { stripCustomer, warningHelp } from "@/lib/mirresOverview";
import type { statusMeta } from "@/lib/mirresStatus";
import type { LineBilling } from "@/lib/tempo_line_contract";

/** Details popover body for one line's Mirres facts. Spans only: it lives inside a <summary>. */
export function MirresPopover({
  id,
  anchor,
  label,
  billing,
  status,
}: {
  id: string;
  anchor: string;
  label: string;
  billing: LineBilling;
  status: ReturnType<typeof statusMeta>;
}) {
  const project = stripCustomer(billing.project, billing.customer);
  const help = warningHelp(billing.warning);
  return (
    <span popover="auto" id={id} className="mirres-popover" role="dialog" aria-label={label}
      style={{ ["positionAnchor" as string]: anchor }}
    >
      <span className="mirres-pop-head">
        <MirresStatusIcon kind={status.kind} size={16} />
        <strong>{billing.customer ?? "Unknown customer"}</strong>
        {project && <span className="mirres-pop-muted">{project}</span>}
      </span>
      <span className="mirres-pop-status">
        {status.label}
        {billing.project_type && <span className="mirres-pop-muted"> · {billing.project_type}</span>}
      </span>
      {billing.warning && (
        <span className="mirres-pop-warning">
          <ContractMissingIcon size={16} />
          <span>
            <span className="mirres-pop-warning-text">{billing.warning}</span>
            {help && <span className="mirres-pop-muted mirres-pop-help">{help}</span>}
          </span>
        </span>
      )}
      <BillingDetails details={billing.details} className="mirres-pop-details" />
      <span className="mirres-pop-key">{billing.account_key}</span>
    </span>
  );
}
