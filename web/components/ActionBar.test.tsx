// Pure-function tests for ActionBar's result summariser (FR-35): the
// estimate result must call out billing-line texts that failed to
// regenerate, not just the block estimate counts.

import { describe, expect, test } from "bun:test";
import { summarise } from "./ActionBar";

describe("summarise", () => {
  test("estimate result with no line_texts reads as before", () => {
    expect(summarise({ estimated: 2, skipped: 1, failed: 0 })).toBe(
      "2 estimated · 1 skipped · 0 failed",
    );
  });

  test("estimate result with an empty not_generated list adds nothing", () => {
    const r = { estimated: 2, skipped: 0, failed: 0, line_texts: { generated: 2, not_generated: [] } };
    expect(summarise(r)).toBe("2 estimated · 0 skipped · 0 failed");
  });

  test("estimate result names each not-regenerated line and its reason", () => {
    const r = {
      estimated: 2,
      skipped: 0,
      failed: 0,
      line_texts: {
        generated: 0,
        not_generated: [
          { folder: "vitinn-infra", customer: "Acme", reason: "provider not configured" },
        ],
      },
    };
    const s = summarise(r);
    expect(s).toContain("2 estimated");
    expect(s).toContain("1 line texts not regenerated");
    expect(s).toContain("vitinn-infra (provider not configured)");
  });

  test("truncates a long not_generated list with an ellipsis", () => {
    const notGenerated = Array.from({ length: 5 }, (_, i) => ({
      folder: `folder-${i}`,
      customer: "Acme",
      reason: "err",
    }));
    const s = summarise({ estimated: 0, skipped: 0, failed: 0, line_texts: { generated: 0, not_generated: notGenerated } });
    expect(s).toContain("5 line texts not regenerated");
    expect(s).toContain("folder-0 (err)");
    expect(s.endsWith("…")).toBe(true);
  });

  test("a dry run counts the Tempo lines it would send (the daemon never counts them as synced)", () => {
    const r = {
      dry_run: true,
      synced: 0,
      skipped: 1,
      errors: [],
      results: [
        { block_id: 1, status: "dry-run" },
        { block_id: 2, status: "dry-run-aggregated" },
        { block_id: 3, status: "dry-run-update" },
        { block_id: 4, status: "skipped" },
      ],
    };
    expect(summarise(r)).toBe("2 to sync · 1 skipped · 0 errors (dry-run)");
    expect(summarise({ dry_run: true, synced: 0, skipped: 0, errors: [], results: [] })).toBe("nothing to preview");
  });

  test("other action shapes are unaffected", () => {
    expect(summarise({ blocks: 3, minutes: 90 })).toBe("3 blocks · 90 min");
  });
});
