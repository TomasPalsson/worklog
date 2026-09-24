"use client";

// All state + mutations behind the Billing registry page. Split out of
// BillingRegistry.tsx to keep that component under the size gate.

import { useCallback, useEffect, useState } from "react";
import {
  deleteBillingCustomer,
  deleteBillingFolder,
  fetchBillingRegistry,
  saveBillingCustomer,
  saveBillingFolder,
} from "@/app/actions";
import { toast } from "./toast";
import type { UnmappedFolder } from "./types";
import {
  type CustomerDraft,
  customerSavePayload,
  type FolderDraft,
  focusJustAddedRow,
  folderSavePayload,
  isFolderQueued,
  newCustomerDraft,
  newFolderDraft,
} from "./billingRegistryDrafts";

let seq = 0;
const nextKey = () => `new-${seq++}`;

type MutateResult = { ok: true } | { ok: false; error: string };

function useRegistryData() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [customers, setCustomers] = useState<CustomerDraft[]>([]);
  const [folders, setFolders] = useState<FolderDraft[]>([]);
  const [unmapped, setUnmapped] = useState<UnmappedFolder[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    const r = await fetchBillingRegistry();
    setLoading(false);
    if (!r.ok) {
      setError(r.error);
      return;
    }
    setCustomers(r.data.customers.map((c) => ({ ...c, key: `c${c.id}` })));
    setFolders(r.data.folders.map((f) => ({ ...f, key: `f${f.id}` })));
    setUnmapped(r.data.unmapped);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return { loading, error, customers, setCustomers, folders, setFolders, unmapped, load };
}

type Mutate = (key: string, label: string, fn: () => Promise<MutateResult>) => Promise<void>;

function useFolderActions(
  data: Pick<ReturnType<typeof useRegistryData>, "folders" | "setFolders">,
  mutate: Mutate,
  onAdded: (key: string) => void,
) {
  function addFolder(folder = "") {
    const key = nextKey();
    data.setFolders((fs) => [...fs, newFolderDraft(key, folder)]);
    onAdded(key);
    if (folder) toast.ok(`Mapping started for ${folder} — pick a customer`);
  }

  function patchFolder(key: string, patch: Partial<FolderDraft>) {
    data.setFolders((fs) => fs.map((f) => (f.key === key ? { ...f, ...patch } : f)));
  }

  function saveFolder(f: FolderDraft) {
    void mutate(f.key, `Saved ${f.folder}`, () => saveBillingFolder(folderSavePayload(f)));
  }

  function deleteFolder(f: FolderDraft) {
    if (f.id == null) {
      data.setFolders((fs) => fs.filter((x) => x.key !== f.key));
      return;
    }
    void mutate(f.key, `Deleted ${f.folder}`, () => deleteBillingFolder(f.id as number));
  }

  return { addFolder, patchFolder, saveFolder, deleteFolder };
}

function useCustomerActions(
  data: Pick<ReturnType<typeof useRegistryData>, "customers" | "setCustomers">,
  mutate: Mutate,
  onAdded: (key: string) => void,
) {
  function addCustomer() {
    const key = nextKey();
    data.setCustomers((cs) => [...cs, newCustomerDraft(key)]);
    onAdded(key);
  }

  function patchCustomer(key: string, patch: Partial<CustomerDraft>) {
    data.setCustomers((cs) => cs.map((c) => (c.key === key ? { ...c, ...patch } : c)));
  }

  function saveCustomer(c: CustomerDraft) {
    void mutate(c.key, `Saved ${c.name}`, () => saveBillingCustomer(customerSavePayload(c)));
  }

  function deleteCustomer(c: CustomerDraft) {
    if (c.id == null) {
      data.setCustomers((cs) => cs.filter((x) => x.key !== c.key));
      return;
    }
    void mutate(c.key, `Deleted ${c.name}`, () => deleteBillingCustomer(c.id as number));
  }

  return { addCustomer, patchCustomer, saveCustomer, deleteCustomer };
}

/** All state + mutations behind the Billing registry page (spec 002's
 * folder mappings + customers, unmapped-folder discovery). */
export function useBillingRegistry() {
  const data = useRegistryData();
  const [busy, setBusy] = useState<string | null>(null);
  // Key of the row just added — flashed and focused so a click deep down
  // a long page doesn't look like it did nothing.
  const [justAdded, setJustAdded] = useState<string | null>(null);

  useEffect(() => {
    if (!justAdded) return;
    focusJustAddedRow(justAdded);
    const t = setTimeout(() => setJustAdded(null), 1400);
    return () => clearTimeout(t);
  }, [justAdded]);

  /** Run one row mutation, then refresh so ids and ordering stay truthful. */
  async function mutate(key: string, label: string, fn: () => Promise<MutateResult>) {
    setBusy(key);
    const r = await fn();
    setBusy(null);
    if (!r.ok) {
      toast.error(`${label} failed — ${r.error}`);
      return;
    }
    toast.ok(label);
    await data.load();
  }

  const folderActions = useFolderActions(data, mutate, setJustAdded);
  const customerActions = useCustomerActions(data, mutate, setJustAdded);

  return {
    loading: data.loading,
    error: data.error,
    busy,
    customers: data.customers,
    folders: data.folders,
    unmapped: data.unmapped,
    justAdded,
    load: data.load,
    isQueued: (folder: string) => isFolderQueued(data.folders, folder),
    ...folderActions,
    ...customerActions,
  };
}
