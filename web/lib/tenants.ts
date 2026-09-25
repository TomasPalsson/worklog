// Types mirror rust/crates/worklog-core/src/tenant_contract.rs; daemon
// calls behind the Billing panel's tenant list + link editor (spec 005).

import { call } from "./daemon";

/** How a tenant got its customer (`tenant_contract::TenantOrigin`). */
export type TenantOrigin = "alias" | "link" | "unmatched" | "ignored";

/** One tenant directory found under a multi-tenant folder's tenant roots
 * (`GET /billing/tenants`). `customer` is `null` for `unmatched`/`ignored`. */
export interface Tenant {
  folder: string;
  name: string;
  customer: string | null;
  origin: TenantOrigin;
}

/** Body of `POST /billing/tenants/link`. `customer: null` + `ignored: false`
 * removes the link (back to alias matching). */
export interface TenantLink {
  folder: string;
  tenant: string;
  customer: string | null;
  ignored: boolean;
}

export async function listTenants(): Promise<Tenant[]> {
  return call<Tenant[]>("GET", "/billing/tenants");
}

export async function linkTenant(link: TenantLink): Promise<{ ok: true }> {
  return call("POST", "/billing/tenants/link", link);
}

/** Where a block's split came from (`tenant_contract::SplitOrigin`). */
export type SplitOrigin = "clues" | "manual" | "fallback";

/** One customer's part of one block (`GET /blocks/:id/customer-slices`).
 * `intervals` are `[start, end)` epoch seconds inside the block. An empty
 * array means the block's folder isn't multi-tenant — render nothing. */
export interface CustomerSlice {
  customer: string | null;
  intervals: [number, number][];
  origin: SplitOrigin;
}

export async function getCustomerSlices(blockId: number): Promise<CustomerSlice[]> {
  return call<CustomerSlice[]>("GET", `/blocks/${blockId}/customer-slices`);
}

/** Save the Owner's hand-set split. `shares` are fractions in `(0, 1]`
 * summing to 1 — the daemon rejects anything else with a 400. */
export async function saveCustomerShares(
  blockId: number,
  shares: Record<string, number>,
): Promise<{ ok: true }> {
  return call("POST", `/blocks/${blockId}/customer-shares`, { shares });
}

/** Drop a hand-set split — the block goes back to the automatic split. */
export async function clearCustomerShares(blockId: number): Promise<{ ok: true }> {
  return call("POST", `/blocks/${blockId}/customer-shares/clear`);
}
