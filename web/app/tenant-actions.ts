"use server";

// Server Actions for the Billing panel's tenant list (spec 005, B16). Split
// out of app/actions.ts so that file's task doesn't need to touch it.

import {
  listTenants as daemonListTenants,
  linkTenant as daemonLinkTenant,
  clearCustomerShares as daemonClearCustomerShares,
} from "@/lib/tenants";
import type { Tenant, TenantLink } from "@/lib/tenants";
import { call } from "@/lib/daemon";
import type { BillingSlice, ShareRow } from "@/lib/deildir";
import type { ActionResult } from "./actions";

async function run<T>(fn: () => Promise<T>): Promise<ActionResult<T>> {
  try {
    return { ok: true, data: await fn() };
  } catch (e) {
    return { ok: false, error: (e as Error).message || "unknown error" };
  }
}

/** Every multi-tenant folder's tenant directories. Read-only, no
 * revalidate — the panel refetches itself after a link. */
export async function fetchTenants(): Promise<ActionResult<Tenant[]>> {
  return run(() => daemonListTenants());
}

/** Link (or clear) a tenant's customer. `customer: null` + `ignored: false`
 * resets it back to automatic alias matching. */
export async function saveTenantLink(link: TenantLink): Promise<ActionResult<{ ok: true }>> {
  return run(() => daemonLinkTenant(link));
}

/** A block's customer split for the card's split line + editor (B17, spec
 * 006 super blocks) — one slice per (customer, deild), returned for every
 * non-personal block. */
export async function fetchCustomerSlices(blockId: number): Promise<ActionResult<BillingSlice[]>> {
  return run(() => call<BillingSlice[]>("GET", `/blocks/${blockId}/customer-slices`));
}

/** Save the Owner's hand-set split rows, in fractions derived from the
 * editor's percent inputs (FR-04/FR-05). */
export async function saveCustomerShares(
  blockId: number,
  rows: ShareRow[],
): Promise<ActionResult<{ ok: true }>> {
  return run(() => call<{ ok: true }>("POST", `/blocks/${blockId}/customer-shares`, { rows }));
}

/** Clear a hand-set split — the block goes back to the automatic split. */
export async function clearCustomerShares(blockId: number): Promise<ActionResult<{ ok: true }>> {
  return run(() => daemonClearCustomerShares(blockId));
}
