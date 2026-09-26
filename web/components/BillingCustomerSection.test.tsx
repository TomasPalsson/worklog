// Spec 006, Journey 1 (B1): the Owner keeps a deildir list per customer.
// Happy path — a deild is added under a customer and survives a reload of
// the registry. Error path — a duplicate name is refused with a visible
// message and the list is left unchanged.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { subscribe, type ToastMsg } from "@/lib/toast";
import type { Deild } from "@/lib/deildir";
import type { CustomerDraft } from "@/lib/billingRegistryDrafts";

let registryDeildir: Deild[] = [];
let nextId = 1;

const fetchBillingRegistryImpl = mock(async () => ({
  ok: true as const,
  data: { customers: [], folders: [], unmapped: [], deildir: registryDeildir },
}));

type SaveResult = { ok: true; data: { id: number } } | { ok: false; error: string };
let saveDeildResult: SaveResult | null = null; // null = default "insert into registryDeildir"
const saveDeildCalls: Deild[] = [];
const saveDeildImpl = mock(async (d: Deild) => {
  saveDeildCalls.push(d);
  if (saveDeildResult) return saveDeildResult;
  const id = d.id ?? nextId++;
  registryDeildir = [
    ...registryDeildir.filter((x) => x.id !== id),
    { id, customer: d.customer, name: d.name, keywords: d.keywords },
  ];
  return { ok: true as const, data: { id } };
});

const deleteDeildCalls: number[] = [];
const deleteDeildImpl = mock(async (id: number) => {
  deleteDeildCalls.push(id);
  registryDeildir = registryDeildir.filter((x) => x.id !== id);
  return { ok: true as const, data: { removed: true } };
});

// CustomerSection pulls its deildir list through `useDeildir`, which talks
// to @/app/actions directly — mock that boundary so no real daemon call (or
// Next.js server-action runtime) is needed, mirroring BillingTenantSection.
// `useDeildir` shares a module (web/lib/useBillingRegistry.ts) with the
// customer/folder actions, so every named import that file makes from
// @/app/actions has to resolve even though this test only exercises deild.
mock.module("@/app/actions", () => ({
  fetchBillingRegistry: () => fetchBillingRegistryImpl(),
  saveDeild: (d: Deild) => saveDeildImpl(d),
  deleteDeild: (id: number) => deleteDeildImpl(id),
  saveBillingCustomer: async () => ({ ok: true as const, data: { id: 1 } }),
  deleteBillingCustomer: async () => ({ ok: true as const, data: { removed: true } }),
  saveBillingFolder: async () => ({ ok: true as const, data: { id: 1 } }),
  deleteBillingFolder: async () => ({ ok: true as const, data: { removed: true } }),
}));

let CustomerSection: (props: {
  customers: CustomerDraft[];
  busy: string | null;
  justAdded: string | null;
  onPatch: (key: string, patch: Partial<CustomerDraft>) => void;
  onSave: (customer: CustomerDraft) => void;
  onDelete: (customer: CustomerDraft) => void;
  onAdd: () => void;
}) => React.JSX.Element;

beforeAll(async () => {
  const mod = await import("./BillingCustomerSection");
  CustomerSection = mod.CustomerSection;
});

afterEach(() => {
  cleanup();
  fetchBillingRegistryImpl.mockClear();
  saveDeildImpl.mockClear();
  deleteDeildImpl.mockClear();
  saveDeildCalls.length = 0;
  deleteDeildCalls.length = 0;
  registryDeildir = [];
  saveDeildResult = null;
  nextId = 1;
});

const sjukra: CustomerDraft = { key: "c1", id: 1, name: "Sjúkra", aliases: [] };

function renderSection() {
  render(
    <CustomerSection
      customers={[sjukra]}
      busy={null}
      justAdded={null}
      onPatch={() => {}}
      onSave={() => {}}
      onDelete={() => {}}
      onAdd={() => {}}
    />,
  );
}

describe("CustomerSection deildir editor (B1)", () => {
  it("adds a deild under a customer and it survives a reload (Journey 1 happy)", async () => {
    renderSection();
    await waitFor(() => expect(fetchBillingRegistryImpl).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByText("Add deild"));

    fireEvent.change(screen.getByLabelText("Deild name"), {
      target: { value: "Rekstur" },
    });
    fireEvent.change(screen.getByLabelText("Deild keywords"), {
      target: { value: "rekstur" },
    });
    fireEvent.click(screen.getByLabelText("Save deild Rekstur"));

    await waitFor(() => expect(saveDeildCalls.length).toBe(1));
    expect(saveDeildCalls[0]).toEqual({
      id: undefined,
      customer: "Sjúkra",
      name: "Rekstur",
      keywords: ["rekstur"],
    });

    // The row survives the post-save reload of the registry.
    await waitFor(() => expect(fetchBillingRegistryImpl).toHaveBeenCalledTimes(2));
    expect((screen.getByLabelText("Deild name") as HTMLInputElement).value).toBe("Rekstur");
  });

  it("refuses a duplicate name with a visible message and leaves the list unchanged (Journey 1 error)", async () => {
    registryDeildir = [{ id: 1, customer: "Sjúkra", name: "Rekstur", keywords: [] }];
    renderSection();
    await screen.findByLabelText("Deild name");

    saveDeildResult = {
      ok: false,
      error: "deild 'Rekstur' already exists for customer 'Sjúkra'",
    };

    const messages: ToastMsg[] = [];
    const unsubscribe = subscribe((msgs) => {
      messages.length = 0;
      messages.push(...msgs);
    });

    fireEvent.click(screen.getByText("Add deild"));
    const nameInputs = screen.getAllByLabelText("Deild name");
    expect(nameInputs.length).toBe(2);
    fireEvent.change(nameInputs[1]!, { target: { value: "Rekstur" } });
    const saveButtons = screen.getAllByLabelText("Save deild Rekstur");
    expect(saveButtons.length).toBe(2);
    fireEvent.click(saveButtons[1]!);

    await waitFor(() => expect(saveDeildCalls.length).toBe(1));
    await waitFor(() =>
      expect(messages.some((m) => m.tone === "error" && m.text.includes("already exists"))).toBe(
        true,
      ),
    );
    unsubscribe();

    // Unchanged: still exactly the original row plus the untouched draft —
    // nothing was persisted, nothing was dropped.
    expect(screen.getAllByLabelText("Deild name").length).toBe(2);
    expect(registryDeildir.length).toBe(1);
  });
});
