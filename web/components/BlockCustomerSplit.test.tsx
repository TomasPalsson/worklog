// B17/B4/B5: a block card shows its computed customer split + origin
// (FR-14) and lets the Owner hand-set it by rows of (customer, deild, %),
// repeating a customer (FR-04/FR-05, spec 006). The split editor is
// available on every non-personal block now — an empty/failed slices fetch
// just means nothing to show yet, not "not multi-tenant". Fetched lazily on
// mount so the day's card list never waits.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BillingSlice, ShareRow } from "@/lib/deildir";
import type { BillingCustomer } from "@/lib/types";

const splitSlices: BillingSlice[] = [
  { customer: "Sjúkra", deild: null, intervals: [[0, 2520]], origin: "clues", deild_origin: "blank" },
  { customer: "MMS", deild: null, intervals: [[2520, 3600]], origin: "clues", deild_origin: "blank" },
];

let currentSlices: BillingSlice[] = splitSlices;

const fetchCustomerSlicesImpl = mock(async (_blockId: number) => ({
  ok: true as const,
  data: currentSlices,
}));

const saveCustomerSharesCalls: [number, ShareRow[]][] = [];
let saveCustomerSharesResult: { ok: true } | { ok: false; error: string } = { ok: true };
const saveCustomerSharesImpl = mock(async (blockId: number, rows: ShareRow[]) => {
  saveCustomerSharesCalls.push([blockId, rows]);
  return saveCustomerSharesResult;
});

const clearCustomerSharesImpl = mock(async (_blockId: number) => ({ ok: true as const }));

// The full registry — every customer the Owner has set up, not just ones
// already linked as tenants, plus every customer's deildir. "Byko" is
// deliberately not one of the block's own slice customers, to prove the
// Add-customer list comes from here.
const registryCustomers: BillingCustomer[] = [
  { id: 1, name: "Sjúkra", aliases: [] },
  { id: 2, name: "MMS", aliases: [] },
  { id: 3, name: "Byko", aliases: [] },
  { id: 4, name: "APRÓ", aliases: [] },
];
const registryDeildir = [
  { id: 1, customer: "Sjúkra", name: "Rekstur", keywords: [] },
  { id: 2, customer: "Sjúkra", name: "Áskrift", keywords: [] },
  { id: 3, customer: "APRÓ", name: "AI hraðall", keywords: [] },
];
const fetchBillingRegistryImpl = mock(async () => ({
  ok: true as const,
  data: { customers: registryCustomers, folders: [], unmapped: [], deildir: registryDeildir },
}));

// BlockCustomerSplit talks to @/app/tenant-actions + @/app/actions directly
// — mock those boundaries so no real daemon call (or Next.js server-action
// runtime) is needed. Bun's `mock.module` replaces the module for the whole
// test run, not just this file, so this also has to cover the exports
// BillingTenantSection.test.tsx needs from the same specifier — otherwise
// whichever file's mock registers first "wins" for every importer and the
// other file's tests fail to link.
mock.module("@/app/tenant-actions", () => ({
  fetchCustomerSlices: (blockId: number) => fetchCustomerSlicesImpl(blockId),
  saveCustomerShares: (blockId: number, rows: ShareRow[]) => saveCustomerSharesImpl(blockId, rows),
  clearCustomerShares: (blockId: number) => clearCustomerSharesImpl(blockId),
  fetchTenants: async () => ({ ok: true as const, data: [] }),
  saveTenantLink: async () => ({ ok: true as const }),
}));
mock.module("@/app/actions", () => ({
  fetchBillingRegistry: () => fetchBillingRegistryImpl(),
}));

let BlockCustomerSplit: (props: { blockId: number }) => React.JSX.Element | null;

beforeAll(async () => {
  const mod = await import("./BlockCustomerSplit");
  BlockCustomerSplit = mod.BlockCustomerSplit;
});

afterEach(() => {
  cleanup();
  currentSlices = splitSlices;
  fetchCustomerSlicesImpl.mockClear();
  saveCustomerSharesImpl.mockClear();
  saveCustomerSharesCalls.length = 0;
  saveCustomerSharesResult = { ok: true };
  clearCustomerSharesImpl.mockClear();
  fetchBillingRegistryImpl.mockClear();
});

async function openEditor() {
  await screen.findByText("Sjúkra 70% · MMS 30%");
  fireEvent.click(screen.getByRole("button", { name: "Change" }));
  await screen.findByLabelText("Sjúkra share percent");
}

describe("BlockCustomerSplit (B17)", () => {
  it("renders the computed split and its origin", async () => {
    render(<BlockCustomerSplit blockId={101} />);

    expect(await screen.findByText("Sjúkra 70% · MMS 30%")).not.toBeNull();
    expect(screen.getByText("auto")).not.toBeNull();
  });

  it("shows a slice's deild in the display line (Customer·Deild %)", async () => {
    currentSlices = [
      { customer: "Sjúkra", deild: "Rekstur", intervals: [[0, 1800]], origin: "manual", deild_origin: "manual" },
      { customer: "APRÓ", deild: "AI hraðall", intervals: [[1800, 3600]], origin: "manual", deild_origin: "manual" },
    ];
    render(<BlockCustomerSplit blockId={505} />);

    expect(await screen.findByText("Sjúkra·Rekstur 50% · APRÓ·AI hraðall 50%")).not.toBeNull();
    expect(screen.getByText("set by you")).not.toBeNull();
  });

  it("saves the exact rows when the Owner edits shares to 50/50", async () => {
    render(<BlockCustomerSplit blockId={101} />);
    await openEditor();

    fireEvent.change(screen.getByLabelText("Sjúkra share percent"), {
      target: { value: "50" },
    });
    fireEvent.change(screen.getByLabelText("MMS share percent"), {
      target: { value: "50" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveCustomerSharesCalls.length).toBe(1));
    expect(saveCustomerSharesCalls[0]).toEqual([
      101,
      [
        { customer: "Sjúkra", deild: null, fraction: 0.5 },
        { customer: "MMS", deild: null, fraction: 0.5 },
      ],
    ]);
  });

  it("saves a customer split across rows of (customer, deild, %), repeating a customer (FR-04)", async () => {
    currentSlices = [
      { customer: "Sjúkra", deild: "Rekstur", intervals: [[0, 1200]], origin: "clues", deild_origin: "keyword" },
      { customer: "Sjúkra", deild: "Áskrift", intervals: [[1200, 2400]], origin: "clues", deild_origin: "keyword" },
      { customer: "APRÓ", deild: "AI hraðall", intervals: [[2400, 3600]], origin: "clues", deild_origin: "keyword" },
    ];
    render(<BlockCustomerSplit blockId={606} />);

    await screen.findByRole("button", { name: "Change" });
    fireEvent.click(screen.getByRole("button", { name: "Change" }));

    fireEvent.change(await screen.findByLabelText("Sjúkra · Rekstur share percent"), {
      target: { value: "33" },
    });
    fireEvent.change(screen.getByLabelText("Sjúkra · Áskrift share percent"), {
      target: { value: "33" },
    });
    fireEvent.change(screen.getByLabelText("APRÓ · AI hraðall share percent"), {
      target: { value: "34" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveCustomerSharesCalls.length).toBe(1));
    expect(saveCustomerSharesCalls[0]).toEqual([
      606,
      [
        { customer: "Sjúkra", deild: "Rekstur", fraction: 0.33 },
        { customer: "Sjúkra", deild: "Áskrift", fraction: 0.33 },
        { customer: "APRÓ", deild: "AI hraðall", fraction: 0.34 },
      ],
    ]);
  });

  it("shows the error and blocks save when shares don't add up to 100% (total 90, FR-05)", async () => {
    render(<BlockCustomerSplit blockId={101} />);
    await openEditor();

    fireEvent.change(screen.getByLabelText("Sjúkra share percent"), {
      target: { value: "60" },
    });

    await screen.findByText("Shares must add up to 100%");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
    expect(saveCustomerSharesCalls.length).toBe(0);
  });

  it("renders nothing when there are no slices to show", async () => {
    currentSlices = [];
    const { container } = render(<BlockCustomerSplit blockId={202} />);

    await waitFor(() => expect(fetchCustomerSlicesImpl).toHaveBeenCalledWith(202));
    expect(container.firstChild).toBeNull();
  });

  it("lists every registry customer in the Add-customer select, not just the block's own slice customers", async () => {
    render(<BlockCustomerSplit blockId={101} />);
    await openEditor();

    const addSelect = screen.getByLabelText("Add customer") as HTMLSelectElement;
    const optionNames = [...addSelect.options].map((o) => o.value).filter(Boolean);
    expect(optionNames).toContain("Byko");
    expect(fetchBillingRegistryImpl).toHaveBeenCalled();
  });

  it("turns a fallback slice with no resolved customer into a required picker and never saves it as Unresolved (FR-05)", async () => {
    currentSlices = [
      { customer: null, deild: null, intervals: [[0, 3600]], origin: "fallback", deild_origin: "blank" },
    ];
    render(<BlockCustomerSplit blockId={303} />);

    await screen.findByText("Unresolved");
    fireEvent.click(screen.getByRole("button", { name: "Change" }));

    const picker = await screen.findByLabelText("Choose a customer");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.change(picker, { target: { value: "MMS" } });

    await waitFor(() =>
      expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(false),
    );

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveCustomerSharesCalls.length).toBe(1));
    expect(saveCustomerSharesCalls[0]).toEqual([303, [{ customer: "MMS", deild: null, fraction: 1 }]]);
  });
});
