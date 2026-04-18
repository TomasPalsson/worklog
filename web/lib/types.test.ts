import { describe, expect, it } from "bun:test";
import { sourceKind } from "./types";

describe("sourceKind", () => {
  it("recognises github sources", () => {
    expect(sourceKind("github_commit")).toBe("github");
    expect(sourceKind("github_pr")).toBe("github");
  });

  it("recognises claude sources", () => {
    expect(sourceKind("claude_prompt")).toBe("claude");
    expect(sourceKind("claude_session_end")).toBe("claude");
  });

  it("recognises calendar sources (both spellings)", () => {
    expect(sourceKind("gcal_event")).toBe("gcal");
    expect(sourceKind("google_calendar")).toBe("gcal");
  });

  it("falls back to other for unknown sources", () => {
    expect(sourceKind("linear")).toBe("other");
    expect(sourceKind("")).toBe("other");
  });
});
