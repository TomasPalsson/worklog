import { ComponentProps, ReactNode } from "react";

import { formatExportHours, reikningshaefi } from "@/lib/export";
import type { BillingCustomer, BillingFolderMap, BillingRow } from "@/lib/types";
import { CustomerPin, DeildMover, VerkefniPin } from "./BillingPins";

interface Props {
  row: BillingRow;
  /** The folder's saved mapping, if any. The pins edit this, not `row`:
   * a row's customer may be resolved from text, and writing that back
   * would pin a guess onto the whole folder. */
  folderPin: BillingFolderMap | null;
  customers: BillingCustomer[];
  /** Verkefni keys already in the registry, offered as suggestions. */
  knownVerkefni: string[];
  /** Every registry customer's deildir names, keyed by customer (FR-13).
   * A line whose customer has any lands the mover instead of the plain
   * folder Verkefni pin. */
  deildirByCustomer?: Record<string, string[]>;
  /** Test-only override for `DeildMover`'s server action call — see its
   * own doc comment. */
  moveLineDeild?: ComponentProps<typeof DeildMover>["moveLineDeild"];
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
export function BillingGroup({
  row,
  folderPin,
  customers,
  knownVerkefni,
  deildirByCustomer = {},
  moveLineDeild,
  children,
}: Props) {
  const needsCustomer = row.customer === null;
  const needsVerkefni = row.verkefni === null;
  const needsInput = needsCustomer || needsVerkefni;
  const blockNoun = row.block_count === 1 ? "block" : "blocks";

  return (
    <details className={`billing-group ${needsInput ? "needs-input" : "complete"}`}>
      <summary>
        <span className="billing-head">
          <PinCells
            row={row}
            folderPin={folderPin}
            customers={customers}
            knownVerkefni={knownVerkefni}
            deildirByCustomer={deildirByCustomer}
            moveLineDeild={moveLineDeild}
          />

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

/** Viðskiptamaður + Verkefni cells: pickers that edit the folder's pin,
 * except Verkefni on a line whose customer has deildir configured — there
 * it moves the whole super block instead (FR-13). */
function PinCells({
  row,
  folderPin,
  customers,
  knownVerkefni,
  deildirByCustomer = {},
  moveLineDeild,
}: Omit<Props, "children">) {
  const pin = {
    folder: row.folder,
    customer: folderPin?.customer ?? null,
    verkefni: folderPin?.verkefni ?? null,
    billable: folderPin?.billable ?? row.billable,
    multi_tenant: folderPin?.multi_tenant ?? false,
  };
  // A multi-tenant split slice billed to another customer isn't this
  // folder's pin, so editing the pin from here would change a different
  // line. That slice is changed on the block instead.
  const editable = !pin.customer || row.customer === null || row.customer === pin.customer;
  const deildOptions = row.customer ? deildirByCustomer[row.customer] ?? [] : [];
  const canMoveDeild = row.customer !== null && deildOptions.length > 0;

  return (
    <>
      <span className="billing-cell">
        {editable ? (
          <CustomerPin {...pin} shown={row.customer} customers={customers} />
        ) : (
          <span className="billing-customer" title="Split from the block — change it there">
            {row.customer}
          </span>
        )}
      </span>

      <span className="billing-cell">
        {canMoveDeild ? (
          <DeildMover
            day={row.day}
            blockIds={row.block_ids}
            customer={row.customer as string}
            current={row.verkefni}
            options={deildOptions}
            moveLineDeild={moveLineDeild}
          />
        ) : editable ? (
          <VerkefniPin {...pin} known={knownVerkefni} />
        ) : (
          <span className="billing-verkefni">{row.verkefni ?? "—"}</span>
        )}
      </span>
    </>
  );
}
