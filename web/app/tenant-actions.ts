"use server";

// Server Actions for the Billing panel's tenant list (spec 005, B16). Split
// out of app/actions.ts so that file's task doesn't need to touch it.

import { listTenants as daemonListTenants, linkTenant as daemonLinkTenant } from "@/lib/tenants";
import type { Tenant, TenantLink } from "@/lib/tenants";
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
