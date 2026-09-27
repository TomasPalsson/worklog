"use client";

import { ComponentProps, ReactNode, useState, useTransition } from "react";
import { useRouter } from "next/navigation";

import {
  regenerateLineText as regenerateLineTextAction,
  saveLineText as saveLineTextAction,
} from "@/app/actions-line-text";
import { formatExportHours, reikningshaefi } from "@/lib/export";
import { toast } from "@/lib/toast";
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
  /** Test-only overrides for the line-text server actions — same reason
   * as `moveLineDeild`. */
  saveLineText?: typeof saveLineTextAction;
  regenerateLineText?: typeof regenerateLineTextAction;
  children: ReactNode;
}

/** FR-35: a line with no stored text ever reads as "not generated" —
 * never implied to be the writer's output. */
function originLabel(origin: BillingRow["text_origin"]): string {
  if (origin === "manual") return "edited by you";
  if (origin === "generated") return "generated";
  return "not generated";
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
  saveLineText = saveLineTextAction,
  regenerateLineText = regenerateLineTextAction,
  children,
}: Props) {
  const router = useRouter();
  const [pending, start] = useTransition();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(row.invoice_text);

  const needsCustomer = row.customer === null;
  const needsVerkefni = row.verkefni === null;
  const needsInput = needsCustomer || needsVerkefni;
  const blockNoun = row.block_count === 1 ? "block" : "blocks";
  const lineKey = { day: row.day, folder: row.folder, customer: row.customer ?? "" };

  function beginEdit() {
    setDraft(row.invoice_text);
    setEditing(true);
  }

  function save() {
    const text = draft;
    start(async () => {
      const r = await saveLineText({ ...lineKey, text });
      if (!r.ok) {
        toast.error(`Couldn't save — ${r.error}`);
        return;
      }
      setEditing(false);
      toast.ok(text.trim() === "" ? "Reset to generated text" : "Saved");
      router.refresh();
    });
  }

  function regenerate() {
    start(async () => {
      const r = await regenerateLineText(lineKey);
      if (!r.ok) {
        toast.error(`Couldn't regenerate — ${r.error}`);
        return;
      }
      if (!r.data.generated) {
        toast.error(`Not regenerated — ${r.data.reason ?? "unknown reason"}`);
        return;
      }
      toast.ok("Regenerated");
      router.refresh();
    });
  }

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

        {/* Clicks here must not toggle the <details> — see PalettePicker's
         * root, which stops propagation for the same reason. */}
        <span className="billing-text-wrap" onClick={(e) => e.stopPropagation()}>
          {editing ? (
            <span className="billing-text-edit">
              <textarea
                aria-label={`Edit invoice text for ${row.folder}`}
                value={draft}
                disabled={pending}
                onChange={(e) => setDraft(e.target.value)}
              />
              <span className="billing-text-edit-actions">
                <button type="button" onClick={save} disabled={pending}>
                  Save
                </button>
                <button type="button" onClick={() => setEditing(false)} disabled={pending}>
                  Cancel
                </button>
              </span>
            </span>
          ) : (
            <>
              <span className="billing-text">{row.invoice_text}</span>
              <span className="billing-text-controls">
                <span
                  className={`billing-chip billing-text-origin billing-text-origin-${row.text_origin ?? "none"}`}
                >
                  {originLabel(row.text_origin ?? null)}
                </span>
                <button type="button" onClick={beginEdit} disabled={pending}>
                  Edit
                </button>
                <button type="button" onClick={regenerate} disabled={pending}>
                  Regenerate
                </button>
              </span>
            </>
          )}
        </span>
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
}: Omit<Props, "children" | "saveLineText" | "regenerateLineText">) {
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
