"use server";

// Server Actions for the Billing panel's tenant list (spec 005, B16). Split
// out of app/actions.ts so that file's task doesn't need to touch it.

import {
  listTenants as daemonListTenants,
  linkTenant as daemonLinkTenant,
  getCustomerSlices as daemonGetCustomerSlices,
  saveCustomerShares as daemonSaveCustomerShares,
  clearCustomerShares as daemonClearCustomerShares,
} from "@/lib/tenants";
import type { CustomerSlice, Tenant, TenantLink } from "@/lib/tenants";
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

/** A block's customer split for the card's split line + editor (B17). Empty
 * array means the block's folder isn't multi-tenant — render nothing. */
export async function fetchCustomerSlices(blockId: number): Promise<ActionResult<CustomerSlice[]>> {
  return run(() => daemonGetCustomerSlices(blockId));
}

/** Save the Owner's hand-set split, in fractions derived from the editor's
 * percent inputs. */
export async function saveCustomerShares(
  blockId: number,
  shares: Record<string, number>,
): Promise<ActionResult<{ ok: true }>> {
  return run(() => daemonSaveCustomerShares(blockId, shares));
}

/** Clear a hand-set split — the block goes back to the automatic split. */
export async function clearCustomerShares(blockId: number): Promise<ActionResult<{ ok: true }>> {
  return run(() => daemonClearCustomerShares(blockId));
}
