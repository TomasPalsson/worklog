import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { getTopics } from "@/lib/daemonStats";
import { timeoutMs } from "@/lib/daemonTimeout";
import type { TopicsReport } from "@/lib/topics_contract";
import { TopicsSection } from "./TopicsSection";

mock.module("next/navigation", () => ({ useRouter: () => ({ refresh: () => {} }) }));

afterEach(cleanup);

const DOWN = "Topic stats need the local helper (`worklog verdict`).";
const FEW = "Not enough described work in this range yet.";

const topic = (id: number, name: string) => ({
  id, name, description: null, named_by: "keywords" as const, keywords: [], seconds: 7200 - id * 3600,
  blocks: 2, days_active: 3, first_day: "2026-10-01", last_day: "2026-10-03", top_tickets: [],
});
const day = (d: string, focus: number | null) => ({
  day: d, work_seconds: 7200, focus, novelty: 0.4, switches: 2, longest_run_seconds: 3600, top_topic: 0, topics_touched: 2,
});
const hl = { block_id: 1, day: "2026-10-02", label: "Fix login", ticket: "ABC-1", topic_id: 0, seconds: 1800, score: 0.5 };

const base: TopicsReport = {
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-03",
  naming: "ai",
  topics: [topic(0, "Fix login"), topic(1, "Billing export")],
  points: [],
  bin: "day",
  river: [
    { start: "2026-10-01", seconds: [3600, 0] },
    { start: "2026-10-02", seconds: [1800, 1800] },
    { start: "2026-10-03", seconds: [0, 3600] },
  ],
  days: [day("2026-10-01", 0.6), day("2026-10-02", 0.7), day("2026-10-03", 0.8)],
  day_similarity: [[1, 0.5, 0.2], [0.5, 1, 0.4], [0.2, 0.4, 1]],
  first_times: [hl],
  oddballs: [hl],
  oddball_median: 0.6,
  embedded_blocks: 6,
  skipped_blocks: 0,
};

const text = (r: TopicsReport) => render(<TopicsSection report={r} />).container.textContent ?? "";

const ROOTS = [".sx-gal", ".hb-rows", ".sx-river", ".sx-mir", ".sx-cmp", ".tm-wrap"];

describe("timeoutMs", () => {
  it("topics route gets 90s; plain /stats stays 30s", () => {
    expect(timeoutMs("/stats/topics?from=a&to=b")).toBe(90_000);
    expect(timeoutMs("/stats?from=a")).toBe(30_000);
  });
});

describe("getTopics", () => {
  it("GETs /stats/topics with from/to and omits absent params", async () => {
    const paths: string[] = [];
    // mock.module is process-wide: put back whatever was installed before.
    const prev = { ...(await import("@/lib/daemon")) };
    mock.module("@/lib/daemon", () => ({
      ...prev,
      call: async (_method: string, path: string) => {
        paths.push(path);
        return base;
      },
    }));
    try {
      const r = await getTopics("2026-10-01", "2026-10-07");
      await getTopics();
      expect(r.status).toBe("ready");
    } finally {
      mock.module("@/lib/daemon", () => prev);
    }
    expect(paths).toEqual(["/stats/topics?from=2026-10-01&to=2026-10-07", "/stats/topics"]);
  });
});

describe("TopicsSection", () => {
  // catches: null falling through to the ready branch, or a missing telescope art
  it("null report and helper_down show the telescope and helper message only", () => {
    for (const report of [null, { ...base, status: "helper_down" as const }]) {
      const { getByText, getByRole, container } = render(<TopicsSection report={report} />);
      expect(getByText(DOWN)).toBeTruthy();
      expect(getByRole("img").getAttribute("aria-label")).toContain("telescope");
      expect(container.textContent).not.toContain("Fix login");
      cleanup();
    }
  });

  // catches: too_few sharing the helper_down message or art
  it("too_few_blocks shows the sparse chart and not-enough message, not the helper one", () => {
    const { getByText, getByRole, container } = render(<TopicsSection report={{ ...base, status: "too_few_blocks" }} />);
    expect(getByText(FEW)).toBeTruthy();
    expect(getByRole("img").getAttribute("aria-label")).toContain("nearly empty");
    expect(container.textContent).not.toContain(DOWN);
  });

  // catches: a module dropped or the order shuffled
  it("ready renders all six module roots in order", () => {
    const { container } = render(<TopicsSection report={base} />);
    const all = Array.from(container.querySelectorAll("*"));
    const idx = ROOTS.map((sel) => {
      const el = container.querySelector(sel);
      expect(el).not.toBeNull();
      return all.indexOf(el!);
    });
    expect(idx).toEqual([...idx].sort((a, b) => a - b));
  });

  // catches: empty states leaking into the ready layout
  it("ready omits the empty states", () => {
    const t = text(base);
    expect(t).not.toContain(DOWN);
    expect(t).not.toContain(FEW);
  });

  // catches: refresh timer always on, or keyed on the wrong naming value
  it("naming pending renders the refresh timer; ai and off do not", () => {
    const has = (naming: TopicsReport["naming"]) => {
      const { container } = render(<TopicsSection report={{ ...base, naming }} />);
      const found = container.querySelector("[data-refresh-soon]") !== null;
      cleanup();
      return found;
    };
    expect([has("pending"), has("ai"), has("off")]).toEqual([true, false, false]);
  });

  // catches: old Card sections surviving the rewrite
  it("output carries no retired drift/sprawl/similar-days text", () => {
    const t = text(base).toLowerCase();
    for (const w of ["drift", "sprawl", "similar_days", "similar days", "catch-all"]) expect(t).not.toContain(w);
  });
});
