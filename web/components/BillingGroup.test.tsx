// A billing line's Viðskiptamaður / Verkefni stay editable after they're set,
// and edits write the folder's real pin (not the line's resolved values) with
// its multi_tenant flag intact.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BillingFolderMap, BillingRow } from "@/lib/types";

const saveBillingFolder = mock(async (_f: BillingFolderMap) => ({
  ok: true as const,
  data: { id: 1 },
}));
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: mock(() => {}) }),
}));
mock.module("@/app/actions", () => ({ saveBillingFolder }));

let BillingGroup: typeof import("./BillingGroup").BillingGroup;

beforeAll(async () => {
  BillingGroup = (await import("./BillingGroup")).BillingGroup;
});

afterEach(() => {
  cleanup();
  saveBillingFolder.mockClear();
});

function row(overrides: Partial<BillingRow>): BillingRow {
  return {
    day: "2026-09-25",
    folder: "vitinn-infra",
    customer: "APRÓ",
    verkefni: "[O] AI hraðall - rekstur",
    ticket: null,
    seconds: 3600,
    hours: 1,
    billable: true,
    invoice_text: "infra",
    needs_description: false,
    block_count: 7,
    started_at: "2026-09-25T09:00:00Z",
    ended_at: "2026-09-25T10:00:00Z",
    ...overrides,
  } as BillingRow;
}

const pin: BillingFolderMap = {
  id: 3,
  folder: "vitinn-infra",
  customer: "APRÓ",
  verkefni: "[O] AI hraðall - rekstur",
  billable: true,
  multi_tenant: true,
};

const customers = [
  { id: 1, name: "APRÓ", aliases: [] },
  { id: 2, name: "Sjúkra", aliases: [] },
];

describe("BillingGroup pins", () => {
  it("lets a set Verkefni be changed, keeping the folder's pin and multi_tenant", async () => {
    render(
      <BillingGroup
        row={row({})}
        folderPin={pin}
        customers={customers}
        knownVerkefni={["[O] AI hraðall - rekstur", "[P] Vöktun"]}
      >
        {null}
      </BillingGroup>,
    );
    fireEvent.click(screen.getByLabelText(/Verkefni for vitinn-infra — currently/));
    fireEvent.click(screen.getByRole("option", { name: "[P] Vöktun" }));
    await waitFor(() => expect(saveBillingFolder).toHaveBeenCalled());
    expect(saveBillingFolder.mock.calls[0][0]).toEqual({
      folder: "vitinn-infra",
      customer: "APRÓ",
      verkefni: "[P] Vöktun",
      billable: true,
      multi_tenant: true,
    });
  });

  it("lets a set customer be changed", () => {
    render(
      <BillingGroup row={row({})} folderPin={pin} customers={customers} knownVerkefni={[]}>
        {null}
      </BillingGroup>,
    );
    expect(screen.getByLabelText("Viðskiptamaður for vitinn-infra — currently APRÓ")).toBeTruthy();
  });

  it("keeps a split slice billed to another customer read-only", () => {
    render(
      <BillingGroup
        row={row({ customer: "Sjúkra", verkefni: null })}
        folderPin={pin}
        customers={customers}
        knownVerkefni={[]}
      >
        {null}
      </BillingGroup>,
    );
    expect(screen.queryByLabelText(/Viðskiptamaður for vitinn-infra/)).toBeNull();
    expect(screen.getByText("Sjúkra")).toBeTruthy();
  });
});
