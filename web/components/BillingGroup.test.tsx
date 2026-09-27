// A billing line's Viðskiptamaður / Verkefni stay editable after they're set,
// and edits write the folder's real pin (not the line's resolved values) with
// its multi_tenant flag intact.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { BillingFolderMap, BillingRow } from "@/lib/types";
import type { LineTextKey, LineTextStatusResult, SetLineTextInput } from "@/lib/daemonLineText";
import { ToastHost } from "./ToastHost";

const saveBillingFolder = mock(async (_f: BillingFolderMap) => ({
  ok: true as const,
  data: { id: 1 },
}));
// Passed straight in as `<BillingGroup moveLineDeild={moveLineDeild}>` (a
// prop, not a `mock.module`): `@/app/tenant-actions` is also mocked by
// unrelated test files, and Bun's `mock.module` replaces a specifier for
// the whole test run, not just this file — a prop sidesteps that collision
// entirely (see `DeildMover`'s doc comment in BillingPins.tsx).
const moveLineDeild = mock(async (_args: unknown) => ({
  ok: true as const,
  data: { ok: true as const },
}));
// Same reasoning as `moveLineDeild` above — a prop override, not a
// `mock.module`, for the line-text server actions.
const saveLineText = mock(async (_i: SetLineTextInput) => ({
  ok: true as const,
  data: { ok: true as const },
}));
const regenerateLineText = mock(async (_i: LineTextKey) => ({
  ok: true as const,
  data: { started: true },
}));
const lineTextStatus = mock(async (_i: LineTextKey): Promise<{ ok: true; data: LineTextStatusResult }> => ({
  ok: true as const,
  data: { state: "done" },
}));
mock.module("next/navigation", () => ({
  useRouter: () => ({ refresh: mock(() => {}) }),
}));
// Process-wide like every mock.module: BlockCard.test mocks this specifier
// too, so export the names its tree imports as well (order must not matter).
const okVoid = () => mock(async () => ({ ok: true as const, data: undefined }));
const okList = () => mock(async () => ({ ok: true as const, data: [] }));
mock.module("@/app/actions", () => ({
  saveBillingFolder,
  setDuration: okVoid(),
  setDescription: okVoid(),
  setPersonal: okVoid(),
  deleteBlock: okVoid(),
  describeBlock: mock(async () => ({ ok: true as const, data: { minutes: 0, jira_issue: null } })),
  fetchBlockEvents: okList(),
  fetchBlockCommits: okList(),
  assignTicket: okVoid(),
  assignExternalTicket: okVoid(),
  searchJiraTickets: okList(),
  createTicket: okVoid(),
  fetchAccounts: okList(),
  fetchProjects: okList(),
}));

let BillingGroup: typeof import("./BillingGroup").BillingGroup;

beforeAll(async () => {
  BillingGroup = (await import("./BillingGroup")).BillingGroup;
});

afterEach(() => {
  cleanup();
  saveBillingFolder.mockClear();
  moveLineDeild.mockClear();
  saveLineText.mockClear();
  regenerateLineText.mockClear();
  lineTextStatus.mockClear();
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
    block_ids: [11, 22],
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

describe("BillingGroup deild mover (FR-13)", () => {
  it("moves every block on the line when the customer has deildir configured", async () => {
    render(
      <BillingGroup
        row={row({})}
        folderPin={pin}
        customers={customers}
        knownVerkefni={["[O] AI hraðall - rekstur", "[P] Vöktun"]}
        deildirByCustomer={{ APRÓ: ["[O] AI hraðall - rekstur", "Rekstur"] }}
        moveLineDeild={moveLineDeild}
      >
        {null}
      </BillingGroup>,
    );
    fireEvent.click(screen.getByLabelText(/Verkefni for APRÓ — currently/));
    fireEvent.click(screen.getByRole("option", { name: "Rekstur" }));
    await waitFor(() => expect(moveLineDeild).toHaveBeenCalled());
    expect(moveLineDeild.mock.calls[0][0]).toEqual({
      day: "2026-09-25",
      blockIds: [11, 22],
      customer: "APRÓ",
      fromDeild: "[O] AI hraðall - rekstur",
      toDeild: "Rekstur",
    });
    // The folder pin action must not fire — this line's own blocks move,
    // not tomorrow's folder default.
    expect(saveBillingFolder).not.toHaveBeenCalled();
  });

  it("clears the deild by picking blank", async () => {
    render(
      <BillingGroup
        row={row({})}
        folderPin={pin}
        customers={customers}
        knownVerkefni={[]}
        deildirByCustomer={{ APRÓ: ["[O] AI hraðall - rekstur", "Rekstur"] }}
        moveLineDeild={moveLineDeild}
      >
        {null}
      </BillingGroup>,
    );
    fireEvent.click(screen.getByLabelText(/Verkefni for APRÓ — currently/));
    fireEvent.click(screen.getByText(/Clear/));
    await waitFor(() => expect(moveLineDeild).toHaveBeenCalled());
    expect(moveLineDeild.mock.calls[0][0]).toEqual({
      day: "2026-09-25",
      blockIds: [11, 22],
      customer: "APRÓ",
      fromDeild: "[O] AI hraðall - rekstur",
      toDeild: null,
    });
  });

  it("falls back to the folder Verkefni pin when the customer has no deildir", async () => {
    render(
      <BillingGroup
        row={row({})}
        folderPin={pin}
        customers={customers}
        knownVerkefni={["[O] AI hraðall - rekstur"]}
      >
        {null}
      </BillingGroup>,
    );
    fireEvent.click(screen.getByLabelText(/Verkefni for vitinn-infra — currently/));
    fireEvent.click(screen.getByRole("option", { name: "[O] AI hraðall - rekstur" }));
    await waitFor(() => expect(saveBillingFolder).toHaveBeenCalled());
    expect(moveLineDeild).not.toHaveBeenCalled();
  });
});

describe("BillingGroup line text (FR-26/FR-31/FR-33/FR-35)", () => {
  function renderGroup(overrides: Partial<BillingRow> = {}) {
    render(
      <BillingGroup
        row={row(overrides)}
        folderPin={pin}
        customers={customers}
        knownVerkefni={[]}
        saveLineText={saveLineText}
        regenerateLineText={regenerateLineText}
        lineTextStatus={lineTextStatus}
        pollIntervalMs={1}
      >
        {null}
      </BillingGroup>,
    );
  }

  it("badges a generated line as 'generated'", () => {
    renderGroup({ text_origin: "generated" });
    expect(screen.getByText("generated")).toBeTruthy();
  });

  it("badges a hand-edited line as 'edited by you'", () => {
    renderGroup({ text_origin: "manual" });
    expect(screen.getByText("edited by you")).toBeTruthy();
  });

  it("badges a line with no stored text as 'not generated' (FR-35)", () => {
    renderGroup({ text_origin: null });
    expect(screen.getByText("not generated")).toBeTruthy();
  });

  it("offers Generate, not Regenerate, when nothing was generated yet", () => {
    renderGroup({ text_origin: null });
    expect(screen.getByRole("button", { name: "Generate" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Regenerate" })).toBeNull();
  });

  it("offers Regenerate once a text exists", () => {
    renderGroup({ text_origin: "generated" });
    expect(screen.getByRole("button", { name: "Regenerate" })).toBeTruthy();
  });

  it("Edit → Save calls saveLineText with {day, folder, customer, text}", async () => {
    renderGroup({ invoice_text: "Old text" });
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Edit invoice text for vitinn-infra"), {
      target: { value: "New text" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(saveLineText).toHaveBeenCalled());
    expect(saveLineText.mock.calls[0][0]).toEqual({
      day: "2026-09-25",
      folder: "vitinn-infra",
      customer: "APRÓ",
      text: "New text",
    });
  });

  it("Cancel discards the edit without saving", () => {
    renderGroup({ invoice_text: "Old text" });
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Edit invoice text for vitinn-infra"), {
      target: { value: "Discarded" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(saveLineText).not.toHaveBeenCalled();
    expect(screen.getByText("Old text")).toBeTruthy();
  });

  it("shows the reason in a toast when the job settles failed", async () => {
    lineTextStatus.mockImplementationOnce(async () => ({
      ok: true as const,
      data: { state: "failed", reason: "hand-edited" } as LineTextStatusResult,
    }));
    render(
      <>
        <ToastHost />
        <BillingGroup
          row={row({ text_origin: "manual" })}
          folderPin={pin}
          customers={customers}
          knownVerkefni={[]}
          saveLineText={saveLineText}
          regenerateLineText={regenerateLineText}
          lineTextStatus={lineTextStatus}
          pollIntervalMs={1}
        >
          {null}
        </BillingGroup>
      </>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Regenerate" }));
    await waitFor(() => expect(regenerateLineText).toHaveBeenCalled());
    await waitFor(() => expect(lineTextStatus).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText(/hand-edited/)).toBeTruthy());
  });

  it("shows a rejection toast when regenerate is already running", async () => {
    regenerateLineText.mockImplementationOnce(async () => ({
      ok: true as const,
      data: { started: false, reason: "already running" },
    }));
    render(
      <>
        <ToastHost />
        <BillingGroup
          row={row({ text_origin: "generated" })}
          folderPin={pin}
          customers={customers}
          knownVerkefni={[]}
          saveLineText={saveLineText}
          regenerateLineText={regenerateLineText}
          lineTextStatus={lineTextStatus}
          pollIntervalMs={1}
        >
          {null}
        </BillingGroup>
      </>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Regenerate" }));
    await waitFor(() => expect(regenerateLineText).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText(/already running/)).toBeTruthy());
    expect(lineTextStatus).not.toHaveBeenCalled();
  });

  it("polls running→done: shows Writing…, disables the button, then toasts and refreshes", async () => {
    lineTextStatus.mockImplementationOnce(async () => ({
      ok: true as const,
      data: { state: "running" } as LineTextStatusResult,
    }));
    render(
      <>
        <ToastHost />
        <BillingGroup
          row={row({ text_origin: "generated" })}
          folderPin={pin}
          customers={customers}
          knownVerkefni={[]}
          saveLineText={saveLineText}
          regenerateLineText={regenerateLineText}
          lineTextStatus={lineTextStatus}
          pollIntervalMs={1}
        >
          {null}
        </BillingGroup>
      </>,
    );
    const button = screen.getByRole("button", { name: "Regenerate" });
    fireEvent.click(button);
    await waitFor(() => expect(screen.getByText("Writing…")).toBeTruthy());
    await waitFor(() => expect(lineTextStatus).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByText("Regenerated")).toBeTruthy());
  });
});
