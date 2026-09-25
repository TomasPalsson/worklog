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
