"use client";

// State + pure helpers behind BlockCustomerSplit, split out to stay under
// the size gate (mirrors useBillingRegistry.ts's split from
// BillingRegistry.tsx). Rows of (customer, deild, %); a customer may repeat
// with a different deild (spec 006, B4/B5/FR-04/FR-05).

import { useEffect, useState, useTransition } from "react";
// Namespace import, not `import { fetchBillingRegistry }`: other test files
// (outside this unit) mock `@/app/actions` with their own partial export
// set, and a named import's static binding check would fail to link
// against that at module-load time. A namespace object degrades to
// `undefined` on a missing key instead, same as any other unmocked call.
import * as billingActions from "@/app/actions";
import {
  clearCustomerShares,
  fetchCustomerSlices,
  saveCustomerShares,
} from "@/app/tenant-actions";
import type { BillingSlice, ShareRow } from "@/lib/deildir";

export interface Part {
  customer: string | null;
  deild: string | null;
  percent: number;
}

export function displayName(customer: string | null): string {
  return customer ?? "Unresolved";
}

/** "Customer·Deild", or just "Customer" when the slice has no deild. */
export function partLabel(part: Part): string {
  const name = displayName(part.customer);
  return part.deild ? `${name}·${part.deild}` : name;
}

/** One part per slice, percent of the block's duration, highest share first. */
export function sliceParts(slices: BillingSlice[]): Part[] {
  const durations = slices.map((s) =>
    s.intervals.reduce((sum, [from, to]) => sum + (to - from), 0),
  );
  const total = durations.reduce((sum, d) => sum + d, 0);
  return slices
    .map((s, i) => ({
      customer: s.customer,
      deild: s.deild,
      percent: total > 0 ? Math.round((durations[i] / total) * 100) : 0,
    }))
    .sort((a, b) => b.percent - a.percent);
}

export function totalSeconds(slices: BillingSlice[]): number {
  return slices.reduce(
    (sum, s) => sum + s.intervals.reduce((a, [from, to]) => a + (to - from), 0),
    0,
  );
}

/** "1h 23m" for the editor's live per-row estimate. */
export function formatApprox(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

export function splitLineText(parts: Part[]): string {
  return parts.length === 1
    ? partLabel(parts[0])
    : parts.map((p) => `${partLabel(p)} ${p.percent}%`).join(" · ");
}

/** `customer: ""` = a Fallback slice with nothing resolved — the row
 * renders a <select> the Owner must fill in; Save stays disabled until
 * every row has one and the total is 100%, so nothing saves "Unresolved".
 * `deild: ""` = no deild, sent to the daemon as `null`. */
export interface Row {
  customer: string;
  deild: string;
  percent: number;
}

export function splitEvenly(rows: Row[]): Row[] {
  const n = rows.length;
  if (n === 0) return rows;
  const base = Math.floor(100 / n);
  const remainder = 100 - base * n;
  return rows.map((r, i) => ({ ...r, percent: base + (i < remainder ? 1 : 0) }));
}

/** Loads the block's slices once on mount. */
function useLoadedSlices(blockId: number) {
  const [slices, setSlices] = useState<BillingSlice[] | null>(null);
  useEffect(() => {
    let alive = true;
    fetchCustomerSlices(blockId).then((r) => {
      if (alive) setSlices(r.ok ? r.data : []);
    });
    return () => {
      alive = false;
    };
  }, [blockId]);
  return [slices, setSlices] as const;
}

export interface CustomerOptions {
  customers: string[];
  deildirByCustomer: Record<string, string[]>;
}

const EMPTY_OPTIONS: CustomerOptions = { customers: [], deildirByCustomer: {} };

/** Every registry customer + their deildir, deferred to editor-open so the
 * card list itself never pays for it. */
function useCustomerOptions(editing: boolean): CustomerOptions {
  const [options, setOptions] = useState<CustomerOptions>(EMPTY_OPTIONS);
  useEffect(() => {
    if (!editing) return;
    let alive = true;
    billingActions.fetchBillingRegistry().then((r) => {
      if (!alive || !r.ok) return;
      const deildirByCustomer: Record<string, string[]> = {};
      for (const d of r.data.deildir) {
        (deildirByCustomer[d.customer] ??= []).push(d.name);
      }
      setOptions({
        customers: r.data.customers.map((c) => c.name).filter((n) => n.trim() !== ""),
        deildirByCustomer,
      });
    });
    return () => {
      alive = false;
    };
  }, [editing]);
  return options;
}

/** State + mutations behind the card, split out to stay under the size gate. */
export function useCustomerSplit(blockId: number) {
  const [slices, setSlices] = useLoadedSlices(blockId);
  const [editing, setEditing] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);
  const customerOptions = useCustomerOptions(editing);
  const [error, setError] = useState<string | null>(null);
  const [isPending, start] = useTransition();

  async function refreshSlices() {
    const fresh = await fetchCustomerSlices(blockId);
    if (fresh.ok) setSlices(fresh.data);
  }

  function openEditor(parts: Part[]) {
    setRows(parts.map((p) => ({ customer: p.customer ?? "", deild: p.deild ?? "", percent: p.percent })));
    setError(null);
    setEditing(true);
  }

  function closeEditor() {
    setEditing(false);
    setError(null);
  }

  function save() {
    const rows_: ShareRow[] = rows.map((r) => ({
      customer: r.customer,
      deild: r.deild || null,
      fraction: r.percent / 100,
    }));
    start(async () => {
      const res = await saveCustomerShares(blockId, rows_);
      if (!res.ok) return setError(res.error);
      await refreshSlices();
      setEditing(false);
    });
  }

  function reset() {
    start(async () => {
      const res = await clearCustomerShares(blockId);
      if (!res.ok) return setError(res.error);
      await refreshSlices();
      setEditing(false);
    });
  }

  return {
    slices,
    editing,
    rows,
    setRows,
    customerOptions,
    error,
    isPending,
    openEditor,
    closeEditor,
    save,
    reset,
  };
}
