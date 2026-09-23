// L7: the confidence badge on each block card shows visible text and an
// aria-label for every confidence level the daemon can send.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
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
