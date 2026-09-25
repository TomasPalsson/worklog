// Pure helpers behind the Billing registry's editable rows — no React
// here, just draft shaping and the payloads the daemon expects. Split out
// so the hook that owns state stays under the size gate.

import type { BillingCustomer, BillingFolderMap } from "./types";

export type FolderDraft = BillingFolderMap & { key: string };
export type CustomerDraft = BillingCustomer & { key: string };

export function newFolderDraft(key: string, folder = ""): FolderDraft {
  return { key, folder, customer: null, verkefni: null, billable: true, multi_tenant: false };
}

export function newCustomerDraft(key: string): CustomerDraft {
  return { key, name: "", aliases: [] };
}

/** A chip whose folder already has an unsaved row below. */
export function isFolderQueued(folders: FolderDraft[], folder: string): boolean {
  return folders.some((f) => f.id == null && f.folder === folder);
}

export function folderSavePayload(f: FolderDraft): BillingFolderMap {
  return {
    id: f.id,
    folder: f.folder,
    customer: f.customer,
    verkefni: f.verkefni,
    billable: f.billable,
    multi_tenant: f.multi_tenant ?? false,
  };
}

export function customerSavePayload(c: CustomerDraft): BillingCustomer {
  return { id: c.id, name: c.name, aliases: c.aliases };
}

/** Scroll a freshly-added row into view and focus the next actual
 * decision: the customer dropdown if the folder is already filled (came
 * from an unmapped-folder chip), otherwise the folder name input. */
export function focusJustAddedRow(key: string) {
  const row = document.querySelector<HTMLElement>(`[data-row-key="${key}"]`);
  if (!row) return;
  const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  row.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "center" });
  const folderInput = row.querySelector<HTMLInputElement>("input");
  const target: HTMLElement | null = folderInput?.value.trim()
    ? (row.querySelector<HTMLSelectElement>("select") ?? folderInput)
    : (folderInput ?? null);
  target?.focus({ preventScroll: true });
}
