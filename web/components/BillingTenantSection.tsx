"use client";

// One compact card per many-customers folder (spec 005, B16): tenants that
// still need a customer come first, everything already resolved collapses
// behind a <details>. Self-loads on mount — like RoutingStatusAndRules, it
// doesn't hook into the registry's save/hydrate cycle, since tenant
// discovery (`billing_tenant_roots`) is independent of a folder's
// `multi_tenant` flag.

import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { fetchTenants, saveTenantLink } from "@/app/tenant-actions";
import type { BillingCustomer } from "@/lib/types";
import type { Tenant, TenantOrigin } from "@/lib/tenants";

const AUTO = "__auto__";
const IGNORE = "__ignored__";

function rowKey(t: Pick<Tenant, "folder" | "name">): string {
  return `${t.folder}::${t.name}`;
}

function selectValue(t: Tenant): string {
  return t.origin === "ignored" ? IGNORE : (t.customer ?? "");
}

function originFor(rawValue: string): TenantOrigin {
  if (rawValue === IGNORE) return "ignored";
  if (rawValue === AUTO) return "unmatched";
  return "link";
}

interface RowState {
  status: "saving" | "saved" | "error";
  error?: string;
}

function TenantRow({
  tenant,
  customers,
  rowState,
  onChange,
}: {
  tenant: Tenant;
  customers: BillingCustomer[];
  rowState: RowState | undefined;
  onChange: (value: string) => void;
}) {
  const hasLink = tenant.origin === "link" || tenant.origin === "ignored";
  const isAlias = tenant.origin === "alias";
  const status = rowState?.status;
  return (
    <div className="tenant-row">
      <span className="tenant-row-name reg-mono" title={tenant.name}>
        {tenant.name}
      </span>
      <select
        className="reg-input tenant-row-select"
        aria-label={`Customer for ${tenant.name}`}
        value={selectValue(tenant)}
        disabled={status === "saving"}
        style={isAlias ? { opacity: 0.65 } : undefined}
        onChange={(e) => onChange(e.target.value)}
      >
        {tenant.origin === "unmatched" && (
          <option value="" disabled>
            Choose a customer…
          </option>
        )}
        {hasLink && <option value={AUTO}>Auto</option>}
        {customers
          .filter((c) => c.name.trim() !== "")
          .map((c) => (
            <option key={c.name} value={c.name}>
              {c.name}
            </option>
          ))}
        <option value={IGNORE}>Not a customer</option>
      </select>
      <span className="tenant-row-status">
        {isAlias && status === undefined && <span className="settings-hint">auto</span>}
        {status === "saving" && <Loader2 className="spin" size={12} />}
        {status === "saved" && <span className="settings-hint">Saved</span>}
        {status === "error" && (
          <span className="export-error" role="alert">
            {rowState?.error}
          </span>
        )}
      </span>
    </div>
  );
}

function FolderCard({
  folder,
  tenants,
  customers,
  rows,
  onChange,
}: {
  folder: string;
  tenants: Tenant[];
  customers: BillingCustomer[];
  rows: Record<string, RowState>;
  onChange: (tenant: Tenant, value: string) => void;
}) {
  const needsAction = tenants.filter((t) => t.origin === "unmatched");
  const resolved = tenants.filter((t) => t.origin !== "unmatched");

  return (
    <section className="reg-section">
      <h2>{folder}</h2>
      {needsAction.length === 0 ? (
        <p className="settings-hint">All tenants matched ✓</p>
      ) : (
        <>
          <p className="reg-lede">Needs a customer ({needsAction.length})</p>
          {needsAction.map((t) => (
            <TenantRow
              key={rowKey(t)}
              tenant={t}
              customers={customers}
              rowState={rows[rowKey(t)]}
              onChange={(v) => onChange(t, v)}
            />
          ))}
        </>
      )}
      {resolved.length > 0 && (
        <details>
          <summary>Matched ({resolved.length})</summary>
          {resolved.map((t) => (
            <TenantRow
              key={rowKey(t)}
              tenant={t}
              customers={customers}
              rowState={rows[rowKey(t)]}
              onChange={(v) => onChange(t, v)}
            />
          ))}
        </details>
      )}
    </section>
  );
}

function groupByFolder(tenants: Tenant[]): Map<string, Tenant[]> {
  const byFolder = new Map<string, Tenant[]>();
  for (const t of tenants) {
    byFolder.set(t.folder, [...(byFolder.get(t.folder) ?? []), t]);
  }
  return byFolder;
}

/** State + mutation behind the panel — split from the component so that
 * function stays under the size gate. */
function useTenantList() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [rows, setRows] = useState<Record<string, RowState>>({});

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    const r = await fetchTenants();
    setLoading(false);
    if (!r.ok) {
      setError(r.error);
      return;
    }
    setTenants(r.data);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleChange(tenant: Tenant, rawValue: string) {
    const key = rowKey(tenant);
    const prev = tenant;
    const optimistic: Tenant = {
      ...tenant,
      customer: rawValue === AUTO || rawValue === IGNORE ? null : rawValue,
      origin: originFor(rawValue),
    };
    setTenants((ts) => ts.map((t) => (rowKey(t) === key ? optimistic : t)));
    setRows((rs) => ({ ...rs, [key]: { status: "saving" } }));

    const r = await saveTenantLink({
      folder: tenant.folder,
      tenant: tenant.name,
      customer: optimistic.customer,
      ignored: rawValue === IGNORE,
    });

    if (!r.ok) {
      setTenants((ts) => ts.map((t) => (rowKey(t) === key ? prev : t)));
      setRows((rs) => ({ ...rs, [key]: { status: "error", error: r.error } }));
      return;
    }

    setRows((rs) => ({ ...rs, [key]: { status: "saved" } }));
    setTimeout(() => {
      setRows((rs) => {
        const { [key]: _drop, ...rest } = rs;
        return rest;
      });
    }, 1500);
    // "Auto" is ambiguous (resolves to alias or unmatched) — reconcile
    // with the server rather than guess.
    if (rawValue === AUTO) void load();
  }

  return { loading, error, tenants, rows, load, handleChange };
}

export function BillingTenantSection({ customers }: { customers: BillingCustomer[] }) {
  const { loading, error, tenants, rows, load, handleChange } = useTenantList();

  if (loading) {
    return (
      <div className="settings-loading">
        <Loader2 className="spin" size={20} />
        <span>Loading tenants…</span>
      </div>
    );
  }

  if (error) {
    return (
      <section className="reg-section">
        <p className="export-error" role="alert">
          Couldn&apos;t load tenants — {error}
        </p>
        <button type="button" className="action-btn" onClick={() => void load()}>
          Retry
        </button>
      </section>
    );
  }

  if (tenants.length === 0) return null;

  return (
    <>
      <section className="reg-section">
        <h2>Tenants</h2>
        <p className="reg-lede">
          Folders inside a many-customers folder. Pick who each one belongs
          to — you only do this once. Missing a customer? Add it under
          Customers above.
        </p>
      </section>
      {[...groupByFolder(tenants).entries()].map(([folder, folderTenants]) => (
        <FolderCard
          key={folder}
          folder={folder}
          tenants={folderTenants}
          customers={customers}
          rows={rows}
          onChange={handleChange}
        />
      ))}
    </>
  );
}
