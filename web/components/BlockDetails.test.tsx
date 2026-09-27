// Details view (spec 006, FR-19..FR-25, FR-34): a filterable timeline
// folding a Claude session into prompt -> tools -> files, with helper
// activity nested inside, every row expandable to its raw JSON.

import { afterEach, beforeAll, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { DetailRow } from "@/lib/clues_contract";

let BlockDetails: (props: { rows: DetailRow[] }) => React.JSX.Element;

beforeAll(async () => {
  const mod = await import("./BlockDetails");
  BlockDetails = mod.BlockDetails;
});

afterEach(() => {
  cleanup();
});

let nextId = 1;
function row(source: string, hhmm: string, title: string, extra: Partial<DetailRow> = {}): DetailRow {
  return {
    id: nextId++,
    source,
    started_at: `2026-09-23T${hhmm}:00Z`,
    title,
    details: null,
    repo: null,
    project_path: null,
    session_id: null,
    jira_issue: null,
    raw: null,
    ...extra,
  };
}

function fixtureRows(): DetailRow[] {
  const prompt1 = row("claude_turn", "09:00", "prompt one", {
    session_id: "s1",
    raw: { kind: "claude_prompt", session_id: "s1", text: "fix the bug" },
  });
  const tool1 = row("claude_tool", "09:01", "tool one", {
    session_id: "s1",
    raw: {
      kind: "claude_tool",
      session_id: "s1",
      tool: "Edit",
      input: { file_path: "a.ts" },
      output: null,
      output_cut_bytes: 0,
      files: ["a.ts"],
    },
  });
  const prompt2 = row("claude_turn", "09:05", "prompt two", {
    session_id: "s1",
    raw: { kind: "claude_prompt", session_id: "s1", text: "now fix the test" },
  });
  const tool2 = row("claude_tool", "09:06", "tool two", {
    session_id: "s1",
    raw: {
      kind: "claude_tool",
      session_id: "s1",
      tool: "Bash",
      input: { command: "bun test" },
      output: null,
      output_cut_bytes: 0,
      files: [],
    },
  });
  const helper1 = row("claude_helper", "09:07", "Subagent: reviewer", {
    session_id: "s1",
    raw: { kind: "helper", parent_session_id: "s1", helper_kind: "subagent", summary: "reviewed the diff" },
  });
  const helper2 = row("claude_helper", "09:08", "Subagent: reviewer", {
    session_id: "s1",
    raw: { kind: "helper", parent_session_id: "s1", helper_kind: "subagent", summary: "approved" },
  });
  const shellRow = row("shell", "08:00", "ls -la", {
    raw: { kind: "shell", command: "ls -la", cwd: "/tmp" },
  });
  const commitRow = row("github_commit", "10:00", "Fix the bug", {
    repo: "acme/widgets",
    raw: { kind: "commit", sha: "abc123", body: "Fix the bug", local_folder: null },
  });
  return [shellRow, prompt1, tool1, prompt2, tool2, helper1, helper2, commitRow];
}

describe("BlockDetails timeline", () => {
  it("folds the Claude session into prompt -> tools -> files, with the helper group nested inside", () => {
    render(<BlockDetails rows={fixtureRows()} />);

    const session = screen.getByText(/Claude session/).closest("details") as HTMLDetailsElement;
    expect(session).toBeTruthy();
    expect(within(session).getByText("fix the bug")).toBeTruthy();
    expect(within(session).getByText("now fix the test")).toBeTruthy();
    expect(within(session).getByText("Edit")).toBeTruthy();
    expect(within(session).getByText("Bash")).toBeTruthy();
    expect(within(session).getByText("Files: a.ts")).toBeTruthy();

    const helperSummary = within(session).getByText("Subagent: reviewer · 2");
    const helperGroup = helperSummary.closest("details") as HTMLDetailsElement;
    expect(helperGroup).toBeTruthy();
    expect(within(helperGroup).getByText("reviewed the diff")).toBeTruthy();
    expect(within(helperGroup).getByText("approved")).toBeTruthy();
  });

  it("shows every filter chip pressed by default", () => {
    render(<BlockDetails rows={fixtureRows()} />);
    expect(screen.getByRole("button", { name: "shell" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "GitHub" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("hides and restores the shell row via its filter chip", () => {
    render(<BlockDetails rows={fixtureRows()} />);
    expect(screen.getByText("ls -la")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "shell" }));
    expect(screen.queryByText("ls -la")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "shell" }));
    expect(screen.getByText("ls -la")).toBeTruthy();
  });

  it("expands a tool row to reveal its stored JSON, including the tool input", () => {
    render(<BlockDetails rows={fixtureRows()} />);
    expect(screen.queryByText(/"file_path": "a.ts"/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /show raw record — Edit/i }));

    expect(screen.getByText(/"file_path": "a.ts"/)).toBeTruthy();
  });

  it("links a github_commit row's title to GitHub when it has a repo and sha", () => {
    render(<BlockDetails rows={fixtureRows()} />);
    const link = screen.getByRole("link", { name: "Fix the bug" });
    expect(link.getAttribute("href")).toBe("https://github.com/acme/widgets/commit/abc123");
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toBe("noreferrer");
  });
});
