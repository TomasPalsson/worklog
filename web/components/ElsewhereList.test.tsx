// The day page's "Done elsewhere" list (spec 006, FR-05/FR-06). Mirrors
// UnsortedList.test.tsx's convention: mock the @/app/actions-elsewhere
// boundary, no real daemon/server-action runtime needed.

import { afterEach, beforeAll, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Block } from "@/lib/types";
import type { ElsewhereItem } from "@/lib/daemonElsewhere";

const item1: ElsewhereItem = {
  id: 1,
  source: "github_commit",
  started_at: "2026-04-18T10:00:00Z",
  title: "fix oauth",
  repo: "aproorg/code-interpreter",
};

const item2: ElsewhereItem = {
  id: 2,
  source: "github_pr",
  started_at: "2026-04-18T11:00:00Z",
  title: "Add retry logic",
  repo: "aproorg/code-interpreter",
};

const block1: Block = {
  id: 10,
  day: "2026-04-18",
  jira_issue: null,
  started_at: "2026-04-18T09:00:00Z",
  ended_at: "2026-04-18T09:30:00Z",
  duration_seconds: 1800,
  description: "Fixed the oauth flow",
  estimated_by: null,
  tempo_worklog_id: null,
  is_personal: false,
  ignored_at: null,
  dirty: false,
  event_count: 2,
  sources: [],
  project_path: "/Users/tomas/Desktop/Work/code-interpreter",
  project: "code-interpreter",
  confidence: "high",
};

const block2: Block = {
  ...block1,
  id: 20,
  started_at: "2026-04-18T13:00:00Z",
  ended_at: "2026-04-18T14:00:00Z",
  description: "vitinn-infra work",
  project: "vitinn-infra",
};

const moveCalls: Array<[number, number, string]> = [];
const moveImpl = mock(async (eventId: number, blockId: number, day: string) => {
  moveCalls.push([eventId, blockId, day]);
  return { ok: true as const, data: undefined };
});

mock.module("@/app/actions-elsewhere", () => ({
  moveElsewhereEvent: (eventId: number, blockId: number, day: string) =>
    moveImpl(eventId, blockId, day),
}));

let ElsewhereList: (props: {
  day: string;
  items: ElsewhereItem[];
  blocks: Block[];
}) => React.JSX.Element | null;

beforeAll(async () => {
  const mod = await import("./ElsewhereList");
  ElsewhereList = mod.ElsewhereList;
});

afterEach(() => {
  cleanup();
  moveImpl.mockClear();
  moveCalls.length = 0;
});

describe("ElsewhereList", () => {
  it("renders nothing for an empty list", () => {
    const { container } = render(
      <ElsewhereList day="2026-04-18" items={[]} blocks={[block1]} />,
    );
    expect(container.innerHTML).toBe("");
  });

  it("renders the fixture items — time, repo, title", () => {
    render(<ElsewhereList day="2026-04-18" items={[item1, item2]} blocks={[block1, block2]} />);
    expect(screen.getByText("fix oauth")).toBeTruthy();
    expect(screen.getByText("Add retry logic")).toBeTruthy();
    expect(screen.getAllByText("aproorg/code-interpreter").length).toBe(2);
  });

  it("moving an item calls the action with (eventId, blockId) and removes the row", async () => {
    render(<ElsewhereList day="2026-04-18" items={[item1]} blocks={[block1, block2]} />);
    expect(screen.getByText("fix oauth")).toBeTruthy();

    fireEvent.change(screen.getByLabelText(/block for fix oauth/i), {
      target: { value: String(block2.id) },
    });
    fireEvent.click(screen.getByRole("button", { name: /move/i }));

    await waitFor(() => expect(moveCalls.length).toBe(1));
    expect(moveCalls).toEqual([[1, 20, "2026-04-18"]]);
    await waitFor(() => expect(screen.queryByText("fix oauth")).toBeNull());
  });
});
