// L7: the confidence badge on each block card shows visible text and an
// aria-label for every confidence level the daemon can send.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { subscribe, type ToastMsg } from "@/lib/toast";
import type { Block, JiraTicket } from "@/lib/types";

mock.module("@/app/actions", () => ({
  setDuration: mock(async () => ({ ok: true as const, data: undefined })),
  setDescription: mock(async () => ({ ok: true as const, data: undefined })),
  setPersonal: mock(async () => ({ ok: true as const, data: undefined })),
  deleteBlock: mock(async () => ({ ok: true as const, data: undefined })),
  describeBlock: mock(async () => ({
    ok: true as const,
    data: { minutes: 0, jira_issue: null },
  })),
  fetchBlockEvents: mock(async () => ({ ok: true as const, data: [] })),
  fetchBlockCommits: mock(async () => ({ ok: true as const, data: [] })),
  assignTicket: mock(async () => ({ ok: true as const, data: undefined })),
  assignExternalTicket: mock(async () => ({ ok: true as const, data: undefined })),
  searchJiraTickets: mock(async () => ({ ok: true as const, data: [] })),
  createTicket: mock(async () => ({ ok: true as const, data: undefined })),
  fetchAccounts: mock(async () => ({ ok: true as const, data: [] })),
  fetchProjects: mock(async () => ({ ok: true as const, data: [] })),
}));

let BlockCard: (props: {
  block: Block;
  tickets: JiraTicket[];
  day: string;
  hideTicketing?: boolean;
  billingCustomer?: string | null;
}) => React.JSX.Element;

beforeAll(async () => {
  const mod = await import("./BlockCard");
  BlockCard = mod.BlockCard;
});

afterEach(() => {
  cleanup();
});

function makeBlock(overrides: Partial<Block>): Block {
  return {
    id: 1,
    day: "2026-07-25",
    jira_issue: null,
    started_at: "2026-07-25T09:00:00Z",
    ended_at: "2026-07-25T09:30:00Z",
    duration_seconds: 1800,
    description: null,
    estimated_by: null,
    tempo_worklog_id: null,
    is_personal: false,
    dirty: false,
    event_count: 3,
    sources: [{ source: "github_commit", n: 3 }],
    project_path: null,
    confidence: "high",
    ...overrides,
  };
}

describe("BlockCard description + clue line", () => {
  it("renders the description as the primary line, ahead of the ticket row", () => {
    render(
      <BlockCard
        block={makeBlock({ description: "Review vitinn-infra PR #802" })}
        tickets={[]}
        day="2026-07-25"
        hideTicketing
      />,
    );
    const body = document.querySelector(".block-body") as HTMLElement;
    const desc = within(body).getByText("Review vitinn-infra PR #802");
    const titleRow = body.querySelector(".block-title-row") as HTMLElement;
    // compareDocumentPosition: DOCUMENT_POSITION_FOLLOWING (4) means titleRow
    // comes after desc in the tree — description is the primary line.
    expect(desc.compareDocumentPosition(titleRow) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('shows "Describing…" when there is no description and no estimated_by', () => {
    render(
      <BlockCard
        block={makeBlock({ description: null, estimated_by: null })}
        tickets={[]}
        day="2026-07-25"
        hideTicketing
      />,
    );
    expect(screen.getByText("Describing…")).toBeTruthy();
  });

  it("shows the existing placeholder when there is no description but the block was estimated", () => {
    render(
      <BlockCard
        block={makeBlock({ description: null, estimated_by: "gap" })}
        tickets={[]}
        day="2026-07-25"
        hideTicketing
      />,
    );
    expect(screen.getByText("Click to add a description…")).toBeTruthy();
  });

  it("builds the clue line from per-source counts with mapped human names, busiest first", () => {
    render(
      <BlockCard
        block={makeBlock({
          sources: [
            { source: "slack", n: 3 },
            { source: "shell", n: 12 },
            { source: "firefox", n: 2 },
            { source: "github_pr", n: 1 },
          ],
        })}
        tickets={[]}
        day="2026-07-25"
        hideTicketing
      />,
    );
    expect(screen.getByText("12 shell · 3 Slack · 2 web · 1 PR")).toBeTruthy();
  });

  it("names the Claude transcript sources in plain words", () => {
    render(
      <BlockCard
        block={makeBlock({
          sources: [
            { source: "claude_work", n: 14 },
            { source: "claude_turn", n: 2 },
          ],
        })}
        tickets={[]}
        day="2026-07-25"
        hideTicketing
      />,
    );
    expect(screen.getByText("14 min Claude working · 2 prompts")).toBeTruthy();
  });

  it("omits the clue line when the block has no sources", () => {
    render(
      <BlockCard block={makeBlock({ sources: [] })} tickets={[]} day="2026-07-25" hideTicketing />,
    );
    expect(document.querySelector(".block-clue-line")).toBeNull();
  });
});

describe("BlockCard confidence badge", () => {
  it.each(["high", "medium", "low"] as const)(
    "shows %s confidence with an aria-label",
    (level) => {
      render(
        <BlockCard
          block={makeBlock({ id: 1, confidence: level })}
          tickets={[]}
          day="2026-07-25"
          hideTicketing
        />,
      );
      const badge = screen.getByLabelText(`${level} confidence`);
      expect(badge.textContent).toBe(`${level} confidence`);
    },
  );
});

describe("BlockCard billing move alert", () => {
  it("tells the owner and shows the card when a description edit moves it to another customer", async () => {
    let toasts: ToastMsg[] = [];
    const unsub = subscribe((m) => (toasts = m));
    const before = makeBlock({ id: 7, description: "infra work" });
    const { rerender } = render(
      <BlockCard block={before} tickets={[]} day="2026-07-25" hideTicketing billingCustomer="Apro" />,
    );
    const desc = screen.getByRole("textbox", { name: /Block description/ });
    desc.innerText = "infra work for sjukra";
    fireEvent.blur(desc);
    await waitFor(() => expect(desc.getAttribute("aria-busy")).toBeNull());

    // The revalidated page re-renders the card under the Sjúkra group.
    rerender(
      <BlockCard
        block={{ ...before, description: "infra work for sjukra" }}
        tickets={[]}
        day="2026-07-25"
        hideTicketing
        billingCustomer="Sjúkra"
      />,
    );
    await waitFor(() => expect(toasts.map((t) => t.text)).toContain("Moved from Apro to Sjúkra"));
    unsub();
  });
});
