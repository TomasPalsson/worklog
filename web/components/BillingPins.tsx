"use client";

// The two inline controls that fill a billing line's blanks: Viðskiptamaður
// and Verkefni. Both are Spotlight-style palettes (see PalettePicker) rather
// than native selects — a system menu looks nothing like this app, and you
// can't filter one.
//
// Both write the same `billing_folder_map` row, keyed on the work folder, so
// each control MUST pass through the other's current value: the daemon upserts
// the whole row, and sending null for the field you aren't editing silently
// wipes it.
//
// They write a folder *mapping*, not a value on this one line — so it fixes
// this line and every future day in that folder. The tooltips say so, because
// a control that quietly changes tomorrow's output too would be a surprise.

import { useRouter } from "next/navigation";
import { useTransition } from "react";

import { saveBillingFolder } from "@/app/actions";
import { toast } from "@/lib/toast";
import type { BillingCustomer } from "@/lib/types";
import { PalettePicker } from "./PalettePicker";

interface PinProps {
  folder: string;
  customer: string | null;
  verkefni: string | null;
  billable: boolean;
}

/** A line with no working directory has no key to hang a mapping on. */
function NoFolder() {
  return (
    <span className="billing-nofolder" title="No working directory on this line">
      no folder
    </span>
  );
}

function isMappable(folder: string): boolean {
  return folder !== "—" && folder.trim() !== "";
}

/** Shared write: always sends the full row so neither field clobbers the other. */
function useSavePin(pin: PinProps) {
  const router = useRouter();
  const [pending, start] = useTransition();

  async function save(patch: Partial<PinProps>, message: string) {
    const r = await saveBillingFolder({
      folder: pin.folder,
      customer: patch.customer !== undefined ? patch.customer : pin.customer,
      verkefni: patch.verkefni !== undefined ? patch.verkefni : pin.verkefni,
      billable: patch.billable !== undefined ? patch.billable : pin.billable,
    });
    if (!r.ok) {
      toast.error(`Couldn't save — ${r.error}`);
      return;
    }
    toast.ok(message);
    start(() => router.refresh());
  }

  return { save, pending };
}

// ───────────────────────────── customer ─────────────────────────────

export function CustomerPin({
  customers,
  ...pin
}: PinProps & { customers: BillingCustomer[] }) {
  const { save, pending } = useSavePin(pin);

  if (!isMappable(pin.folder)) return <NoFolder />;
  if (customers.length === 0) {
    return (
      <a className="billing-nofolder" href="/billing">
        add a customer first
      </a>
    );
  }

  return (
    <PalettePicker
      value={pin.customer}
      options={customers.map((c) => c.name)}
      placeholder="Pick customer"
      searchPlaceholder="Search customers…"
      label={`Viðskiptamaður for ${pin.folder}`}
      tip={`Maps ${pin.folder} → customer, now and for future days`}
      busy={pending}
      onPick={(name) =>
        void save({ customer: name }, `${pin.folder} → ${name} · applies to future days too`)
      }
      onClear={() => void save({ customer: null }, `Cleared customer for ${pin.folder}`)}
    />
  );
}

// ───────────────────────────── verkefni ─────────────────────────────

export function VerkefniPin({ known, ...pin }: PinProps & { known: string[] }) {
  const { save, pending } = useSavePin(pin);

  if (!isMappable(pin.folder)) return <NoFolder />;

  return (
    <PalettePicker
      value={pin.verkefni}
      options={known}
      placeholder="Pick Verkefni"
      searchPlaceholder="Search or paste the key…"
      label={`Verkefni for ${pin.folder}`}
      tip="Paste the accounting key once — remembered for this folder"
      // The valid keys live in the external system, so the first time a
      // project is billed the value has to be typed in.
      allowFree
      busy={pending}
      onPick={(v) => void save({ verkefni: v }, `${pin.folder} → ${v}`)}
      onClear={() => void save({ verkefni: null }, `Cleared Verkefni for ${pin.folder}`)}
    />
  );
}
