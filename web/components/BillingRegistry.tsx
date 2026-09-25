"use client";

// The billing registry — what turns a work folder into a Viðskiptamaður
// and a Verkefni. A full page rather than a modal: it is two tables plus a
// discovery list, which is more than a dialog can show without cramping.
//
// The shortcut that does the real work is "folders seen recently with no
// mapping": remembering which folders you worked in is the tedious part, so
// they are listed with an event count and one click starts a prefilled row.
//
// Rows save individually rather than behind one Save button — each row is
// an independent fact, and a half-finished row must not block saving a
// finished one.
//
// State + mutations live in `useBillingRegistry` and the two table
// sections are their own components — split out so this file stays a
// thin composition of the three.

import { Loader2 } from "lucide-react";
import { saveBillingFolder } from "@/app/actions";
import type { FolderDraft } from "@/lib/billingRegistryDrafts";
import { useBillingRegistry } from "@/lib/useBillingRegistry";
import { BillingTenantSection } from "./BillingTenantSection";
import { CustomerSection } from "./BillingCustomerSection";
import { FolderMappingsSection } from "./BillingFolderSection";

/** Saves immediately (no Save button) — the row's normal Save/Delete
 * pathway (`folderSavePayload`) doesn't carry `multi_tenant`, so this
 * always sends the full row itself. */
async function toggleMultiTenant(
  patchFolder: (key: string, patch: Partial<FolderDraft>) => void,
  folder: FolderDraft,
  checked: boolean,
) {
  patchFolder(folder.key, { multi_tenant: checked });
  const r = await saveBillingFolder({
    id: folder.id,
    folder: folder.folder,
    customer: folder.customer,
    verkefni: folder.verkefni,
    billable: folder.billable,
    multi_tenant: checked,
  });
  if (!r.ok) {
    patchFolder(folder.key, { multi_tenant: !checked });
  }
  return r;
}

export function BillingRegistry() {
  const reg = useBillingRegistry();

  if (reg.loading) {
    return (
      <div className="settings-loading">
        <Loader2 className="spin" size={20} />
        <span>Loading registry…</span>
      </div>
    );
  }

  if (reg.error) {
    return (
      <section className="reg-section">
        <p className="export-error" role="alert">
          Couldn&apos;t load the registry — {reg.error}
        </p>
        <p className="export-hint">Is the worklog daemon running?</p>
        <button type="button" className="action-btn" onClick={() => void reg.load()}>
          Retry
        </button>
      </section>
    );
  }

  return (
    <>
      <FolderMappingsSection
        unmapped={reg.unmapped}
        folders={reg.folders}
        customers={reg.customers}
        busy={reg.busy}
        justAdded={reg.justAdded}
        isQueued={reg.isQueued}
        onPickUnmapped={reg.addFolder}
        onPatch={reg.patchFolder}
        onSave={reg.saveFolder}
        onDelete={reg.deleteFolder}
        onAdd={() => reg.addFolder()}
        onToggleMultiTenant={(folder, checked) =>
          toggleMultiTenant(reg.patchFolder, folder, checked)
        }
      />
      <CustomerSection
        customers={reg.customers}
        busy={reg.busy}
        justAdded={reg.justAdded}
        onPatch={reg.patchCustomer}
        onSave={reg.saveCustomer}
        onDelete={reg.deleteCustomer}
        onAdd={reg.addCustomer}
      />
      <BillingTenantSection customers={reg.customers} />
    </>
  );
}
