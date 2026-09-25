import { describe, expect, it } from "bun:test";
import { folderSavePayload, newFolderDraft, type FolderDraft } from "./billingRegistryDrafts";

describe("folderSavePayload", () => {
  it("carries multi_tenant: true through to the payload", () => {
    const draft: FolderDraft = { key: "k", folder: "x", customer: null, verkefni: null, billable: true, multi_tenant: true };
    expect(folderSavePayload(draft).multi_tenant).toBe(true);
  });

  it("defaults multi_tenant to false when absent on the draft", () => {
    const draft: FolderDraft = { key: "k", folder: "x", customer: null, verkefni: null, billable: true };
    expect(folderSavePayload(draft).multi_tenant).toBe(false);
  });
});

describe("newFolderDraft", () => {
  it("starts with multi_tenant: false", () => {
    expect(newFolderDraft("k").multi_tenant).toBe(false);
  });
});
