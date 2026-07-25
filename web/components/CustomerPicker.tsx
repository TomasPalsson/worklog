"use client";

// Assigns a Viðskiptamaður to a billing line, straight from the day view.
//
// It writes a *folder mapping*, not a one-off value: the customer for a line
// is resolved from the folder registry, so pinning the folder fixes this line
// and every future day in that folder at once. The label says so — a control
// that silently changes tomorrow's output too would be a nasty surprise.
//
// Deliberately a native <select>: it's keyboard- and screen-reader-correct for
// free, and Jakob's Law says a dropdown should behave like a dropdown.

import { useRouter } from "next/navigation";
import { useTransition } from "react";
import { Loader2 } from "lucide-react";

import { saveBillingFolder } from "@/app/actions";
import { toast } from "@/lib/toast";
import type { BillingCustomer } from "@/lib/types";

interface Props {
  /** Work folder this line came from — the key the mapping is written on. */
  folder: string;
  /** Currently resolved customer, or null when nothing matched. */
  current: string | null;
  /** Registered customers to choose from. */
  customers: BillingCustomer[];
}

export function CustomerPicker({ folder, current, customers }: Props) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();

  const noFolder = folder === "—" || folder.trim() === "";

  async function assign(name: string) {
    if (name === "") return;
    // Preserve verkefni/billable by writing only what we know; the daemon
    // upserts on `folder`, so omitted fields are cleared — send them null
    // deliberately rather than by accident.
    const r = await saveBillingFolder({
      folder,
      customer: name,
      verkefni: null,
      billable: true,
    });
    if (!r.ok) {
      toast.error(`Couldn't map ${folder} — ${r.error}`);
      return;
    }
    toast.ok(`${folder} → ${name} · applies to future days too`);
    startTransition(() => router.refresh());
  }

  if (customers.length === 0) {
    return (
      <a className="billing-pick-empty" href="/billing">
        Add a customer first
      </a>
    );
  }

  // A line with no folder has no key to hang a mapping on, so say why rather
  // than offering a control that can't work.
  if (noFolder) {
    return (
      <span className="billing-pick-empty" title="No working directory on this line">
        no folder to map
      </span>
    );
  }

  return (
    <span className="billing-pick">
      {pending && <Loader2 className="spin" size={12} aria-hidden="true" />}
      <select
        className={`billing-pick-select${current === null ? " is-blank" : ""}`}
        aria-label={`Viðskiptamaður for ${folder}`}
        data-tip={`Maps ${folder} → customer, for this and future days`}
        value={current ?? ""}
        disabled={pending}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => void assign(e.target.value)}
      >
        <option value="" disabled>
          Pick Viðskiptamaður…
        </option>
        {customers.map((c) => (
          <option key={c.name} value={c.name}>
            {c.name}
          </option>
        ))}
      </select>
    </span>
  );
}
