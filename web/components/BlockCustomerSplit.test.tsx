// B17: a block card shows its computed customer split + origin (FR-14) and
// lets the Owner hand-set it by percentage (FR-10). Mirrors
// BillingTenantSection.test.tsx's conventions: file-scope mock.module(...),
// then beforeAll(async () => await import(...)).

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { CustomerSlice, Tenant } from "@/lib/tenants";

const splitSlices: CustomerSlice[] = [
  { customer: "Sjúkra", intervals: [[0, 2520]], origin: "clues" },
  { customer: "MMS", intervals: [[2520, 3600]], origin: "clues" },
];

let currentSlices: CustomerSlice[] = splitSlices;

const fetchCustomerSlicesImpl = mock(async (_blockId: number) => ({
  ok: true as const,
  data: currentSlices,
}));

const saveCustomerSharesCalls: [number, Record<string, number>][] = [];
let saveCustomerSharesResult: { ok: true } | { ok: false; error: string } = { ok: true };
const saveCustomerSharesImpl = mock(async (blockId: number, shares: Record<string, number>) => {
  saveCustomerSharesCalls.push([blockId, shares]);
  return saveCustomerSharesResult;
});

const clearCustomerSharesImpl = mock(async (_blockId: number) => ({ ok: true as const }));

const tenants: Tenant[] = [];
const fetchTenantsImpl = mock(async () => ({ ok: true as const, data: tenants }));

// BlockCustomerSplit talks to @/app/tenant-actions directly — mock that
// boundary so no real daemon call (or Next.js server-action runtime) is
// needed.
mock.module("@/app/tenant-actions", () => ({
  fetchCustomerSlices: (blockId: number) => fetchCustomerSlicesImpl(blockId),
  saveCustomerShares: (blockId: number, shares: Record<string, number>) =>
    saveCustomerSharesImpl(blockId, shares),
  clearCustomerShares: (blockId: number) => clearCustomerSharesImpl(blockId),
  fetchTenants: () => fetchTenantsImpl(),
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
  fetchTenantsImpl.mockClear();
});

async function openEditor() {
  await screen.findByText("Sjúkra 70% · MMS 30% · auto");
  fireEvent.click(screen.getByRole("button", { name: "Change" }));
  await screen.findByLabelText("Sjúkra share percent");
}

describe("BlockCustomerSplit (B17)", () => {
  it("renders the computed split and its origin", async () => {
    render(<BlockCustomerSplit blockId={101} />);

    expect(await screen.findByText("Sjúkra 70% · MMS 30% · auto")).not.toBeNull();
  });

  it("saves the exact fractions when the Owner edits shares to 50/50", async () => {
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
    expect(saveCustomerSharesCalls[0]).toEqual([101, { Sjúkra: 0.5, MMS: 0.5 }]);
  });

  it("shows the error and blocks save when shares don't add up to 100%", async () => {
    render(<BlockCustomerSplit blockId={101} />);
    await openEditor();

    fireEvent.change(screen.getByLabelText("Sjúkra share percent"), {
      target: { value: "60" },
    });

    await screen.findByText("Shares must add up to 100%");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
    expect(saveCustomerSharesCalls.length).toBe(0);
  });

  it("renders nothing when the folder isn't multi-tenant (empty slices)", async () => {
    currentSlices = [];
    const { container } = render(<BlockCustomerSplit blockId={202} />);

    await waitFor(() => expect(fetchCustomerSlicesImpl).toHaveBeenCalledWith(202));
    expect(container.firstChild).toBeNull();
  });
});
