import { ReactNode } from "react";

import { formatExportHours, reikningshaefi } from "@/lib/export";
import type { BillingCustomer, BillingRow } from "@/lib/types";
import { CustomerPin, VerkefniPin } from "./BillingPins";

interface Props {
  row: BillingRow;
  customers: BillingCustomer[];
  /** Verkefni keys already in the registry, offered as suggestions. */
  knownVerkefni: string[];
  children: ReactNode;
}

/**
 * One invoice line in the day view — the billing counterpart to `TicketGroup`,
 * and deliberately built from the same card: hairline border, `--radius-lg`,
 * raised surface, 3px state rail on the left (amber = needs input, sage =
 * complete). The day view already speaks in cards, and a different container
 * shape here read as a different app.
 *
 * Inside the card, the two blanks are the controls themselves rather than
 * dashes, and billed hours sit in a fixed right-aligned tabular column so the
 * figures line up down the day.
 */
export function BillingGroup({ row, customers, knownVerkefni, children }: Props) {
  const needsCustomer = row.customer === null;
  const needsVerkefni = row.verkefni === null;
  const needsInput = needsCustomer || needsVerkefni;
  const blockNoun = row.block_count === 1 ? "block" : "blocks";

  const pin = {
    folder: row.folder,
    customer: row.customer,
    verkefni: row.verkefni,
    billable: row.billable,
  };

  return (
    <details className={`billing-group ${needsInput ? "needs-input" : "complete"}`}>
      <summary>
        <span className="billing-head">
          <span className="billing-cell">
            {row.customer ? (
              <span className="billing-customer">{row.customer}</span>
            ) : (
              <CustomerPin {...pin} customers={customers} />
            )}
          </span>

          <span className="billing-cell">
            {row.verkefni ? (
              <span className="billing-verkefni">{row.verkefni}</span>
            ) : (
              <VerkefniPin {...pin} known={knownVerkefni} />
            )}
          </span>

          {/* Fixed, right-aligned, tabular — the figures align down the day. */}
          <span className="billing-cell-hours">
            <span className="billing-hours">{formatExportHours(row.hours)}</span>
            <span className="billing-hours-unit">hrs</span>
          </span>
        </span>

        <span className="billing-sub">
          <span className="billing-folder">{row.folder}</span>
          {row.ticket && <span className="billing-folder">{row.ticket}</span>}
          <span className="billing-blocks">
            {row.block_count} {blockNoun}
          </span>
          {!row.billable && (
            <span className="billing-chip">{reikningshaefi(row.billable)}</span>
          )}
          {row.needs_description && (
            <span className="billing-chip billing-chip-warn">not estimated</span>
          )}
        </span>

        <span className="billing-text">{row.invoice_text}</span>
      </summary>

      <div className="billing-body">{children}</div>
    </details>
  );
}
