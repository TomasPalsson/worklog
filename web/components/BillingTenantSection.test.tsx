// B16: an Unmatched tenant, once the Owner links it to a customer, shows as
// Link (J2 happy); a rejected link (customer no longer exists) leaves it
// Unmatched with the daemon's error shown inline (J2 error). Mirrors
// SettingsPanel.test.tsx's conventions: file-scope mock.module(...), then
// beforeAll(async () => await import(...)).

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BillingCustomer } from "@/lib/types";
import type { Tenant, TenantLink } from "@/lib/tenants";

const customers: BillingCustomer[] = [
  { id: 1, name: "Sjúkra", aliases: [] },
  { id: 2, name: "Byko", aliases: [] },
];

const initialTenants: Tenant[] = [
  { folder: "vitinn-infra", name: "sjukra", customer: "Sjúkra", origin: "alias" },
  { folder: "vitinn-infra", name: "byko-datalake", customer: null, origin: "unmatched" },
];

const fetchTenantsImpl = mock(async () => ({ ok: true as const, data: initialTenants }));
const saveTenantLinkCalls: TenantLink[] = [];
let saveTenantLinkResult: { ok: true } | { ok: false; error: string } = { ok: true };
const saveTenantLinkImpl = mock(async (link: TenantLink) => {
  saveTenantLinkCalls.push(link);
  return saveTenantLinkResult;
});

// BillingTenantSection talks to @/app/tenant-actions directly — mock that
// boundary so no real daemon call (or Next.js server-action runtime) is
// needed.
mock.module("@/app/tenant-actions", () => ({
  fetchTenants: () => fetchTenantsImpl(),
  saveTenantLink: (link: TenantLink) => saveTenantLinkImpl(link),
}));

let BillingTenantSection: (props: { customers: BillingCustomer[] }) => React.JSX.Element | null;

beforeAll(async () => {
  const mod = await import("./BillingTenantSection");
  BillingTenantSection = mod.BillingTenantSection;
});

afterEach(() => {
  cleanup();
  fetchTenantsImpl.mockClear();
  saveTenantLinkImpl.mockClear();
  saveTenantLinkCalls.length = 0;
  saveTenantLinkResult = { ok: true };
});

async function renderPanel() {
  render(<BillingTenantSection customers={customers} />);
  await screen.findByText(/byko-datalake/);
}

describe("BillingTenantSection (B16)", () => {
  it("links an Unmatched tenant to a customer and shows it as matched (J2 happy)", async () => {
    await renderPanel();

    expect(screen.queryByText("Needs a customer (1)")).not.toBeNull();

    fireEvent.change(screen.getByLabelText(/Customer for byko-datalake/i), {
      target: { value: "Byko" },
    });

    await waitFor(() => expect(saveTenantLinkCalls.length).toBe(1));
    expect(saveTenantLinkCalls[0]).toEqual({
      folder: "vitinn-infra",
      tenant: "byko-datalake",
      customer: "Byko",
      ignored: false,
    });

    await waitFor(() =>
      expect(
        (screen.getByLabelText(/Customer for byko-datalake/i) as HTMLSelectElement).value,
      ).toBe("Byko"),
    );
    expect(screen.queryByText(/Needs a customer/)).toBeNull();
    expect(screen.queryByText("All tenants matched ✓")).not.toBeNull();
  });

  it("keeps an Unmatched tenant unmatched and shows the daemon's error when the customer no longer exists (J2 error)", async () => {
    saveTenantLinkResult = { ok: false, error: "Customer no longer exists" };
    await renderPanel();

    fireEvent.change(screen.getByLabelText(/Customer for byko-datalake/i), {
      target: { value: "Byko" },
    });

    await screen.findByText("Customer no longer exists");

    expect(
      (screen.getByLabelText(/Customer for byko-datalake/i) as HTMLSelectElement).value,
    ).toBe("");
    expect(screen.queryByText("Needs a customer (1)")).not.toBeNull();
  });
});
