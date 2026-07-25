import { describe, expect, it } from "bun:test";

import { canMergeGroup, shouldShowSparkles } from "./group-actions";

describe("canMergeGroup", () => {
  // B9: a group with exactly one block has nothing to merge.
  it("B9: hides Merge all when the group has only one block", () => {
    expect(canMergeGroup({ unassigned: false, blocks: [{}] })).toBe(false);
  });

  // B10: the unassigned bucket has no shared ticket; assign first.
  it("B10: hides Merge all on the unassigned bucket even with many blocks", () => {
    expect(canMergeGroup({ unassigned: true, blocks: [{}, {}, {}] })).toBe(false);
  });

  // B8 (predicate side): two assigned same-ticket blocks → show merge.
  it("shows Merge all when an assigned group has 2 blocks", () => {
    expect(canMergeGroup({ unassigned: false, blocks: [{}, {}] })).toBe(true);
  });

  it("shows Merge all when an assigned group has many blocks", () => {
    expect(canMergeGroup({ unassigned: false, blocks: [{}, {}, {}, {}] })).toBe(true);
  });

  it("hides Merge all when an assigned group is empty", () => {
    expect(canMergeGroup({ unassigned: false, blocks: [] })).toBe(false);
  });
});

describe("shouldShowSparkles", () => {
  // B11: assigned solo block — shown.
  it("B11: shows on an assigned block that is sole in its group", () => {
    expect(
      shouldShowSparkles({ jira_issue: "PROJ-1", is_personal: false }, true),
    ).toBe(true);
  });

  // B12: unassigned block — hidden (Claude needs ticket context).
  it("B12: hides on an unassigned block even when sole in its bucket", () => {
    expect(
      shouldShowSparkles({ jira_issue: null, is_personal: false }, true),
    ).toBe(false);
  });

  it("hides when the block is one of several in its group (must merge first)", () => {
    expect(
      shouldShowSparkles({ jira_issue: "PROJ-1", is_personal: false }, false),
    ).toBe(false);
  });

  it("hides on personal blocks — describing personal time isn't useful", () => {
    expect(
      shouldShowSparkles({ jira_issue: "PROJ-1", is_personal: true }, true),
    ).toBe(false);
  });
});
