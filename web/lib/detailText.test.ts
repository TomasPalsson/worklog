import { describe, expect, it } from "bun:test";
import type { DetailRow } from "./clues_contract";
import { foldRepeats, reflogText, rowText, shortPath, toolPreview } from "./detailText";

function row(extra: Partial<DetailRow>): DetailRow {
  return {
    id: 1,
    source: "other",
    started_at: "2026-09-25T10:00:00Z",
    title: "",
    details: null,
    repo: null,
    project_path: null,
    session_id: null,
    jira_issue: null,
    raw: null,
    ...extra,
  };
}

describe("reflogText", () => {
  it("shows a commit's subject, with the verb as detail", () => {
    expect(reflogText("commit: fix the login bug")).toEqual({
      text: "fix the login bug",
      detail: "commit",
      mono: false,
    });
    expect(reflogText("commit (amend): tidy").text).toBe("tidy");
  });

  it("reads a checkout as the branch it switched to", () => {
    expect(reflogText("checkout: moving from main to feat/x")).toEqual({
      text: "Switched to feat/x",
      detail: "from main",
      mono: false,
    });
  });

  it("reads a merge as what was merged", () => {
    expect(reflogText("merge origin/feat/a: Fast-forward").text).toBe("Merged origin/feat/a");
  });
});

describe("rowText", () => {
  it("shows a shell row's full command in mono with a short cwd", () => {
    const t = rowText(
      row({ source: "shell", title: "aws", raw: { kind: "shell", command: "aws sso login", cwd: "/Users/me/Desktop/Work/x" } }),
    );
    expect(t).toEqual({ text: "aws sso login", detail: "~/Desktop/Work/x", mono: true });
  });

  it("tells a Slack channel from a direct message", () => {
    expect(rowText(row({ source: "slack", title: "team-talos", details: "hi" })).text).toBe("#team-talos");
    expect(rowText(row({ source: "slack", title: "Jón Levy", details: "hi" })).text).toBe(
      "Jón Levy · direct message",
    );
  });

  it("shows a web page's host, not its full URL", () => {
    expect(rowText(row({ source: "firefox", title: "Docs", details: "https://a.example/x?q=1" })).detail).toBe(
      "a.example",
    );
  });
});

describe("toolPreview", () => {
  it("picks the meaningful argument per tool", () => {
    expect(toolPreview("Bash", { command: "bun test" })).toBe("bun test");
    expect(toolPreview("Edit", { file_path: "/Users/me/src/a.ts" })).toBe("~/src/a.ts");
    expect(toolPreview("Grep", { pattern: "foo", path: "/Users/me/src" })).toBe("foo  in  ~/src");
    expect(toolPreview("Agent", { description: "review the diff" })).toBe("review the diff");
    expect(toolPreview("WebFetch", { url: "https://docs.example/page" })).toBe("docs.example");
  });
});

describe("foldRepeats", () => {
  it("folds only adjacent repeats, keeping order", () => {
    const folded = foldRepeats(["a", "a", "b", "a"], (s) => s);
    expect(folded).toEqual([
      { item: "a", count: 2 },
      { item: "b", count: 1 },
      { item: "a", count: 1 },
    ]);
  });
});

describe("shortPath", () => {
  it("replaces the home directory with ~", () => {
    expect(shortPath("/Users/tomas/Desktop")).toBe("~/Desktop");
    expect(shortPath("/tmp/x")).toBe("/tmp/x");
  });
});
