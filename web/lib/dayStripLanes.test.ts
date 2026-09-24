import { describe, expect, it } from "bun:test";
import {
  buildLegend,
  buildSegments,
  compactLegend,
  withLegendHues,
  type StripBlock,
} from "./dayStrip";
import { buildLanes, isFocused } from "./dayStripLanes";

// Local-time constructor — same trick as dayStrip.test.ts.
const at = (h: number, m: number) => new Date(2026, 8, 23, h, m).toISOString();

const window = { startMs: new Date(at(9, 0)).getTime(), endMs: new Date(at(13, 0)).getTime() };

const blocks: StripBlock[] = [
  { id: 1, started_at: at(9, 0), ended_at: at(9, 30), project_path: "/x/alpha", is_personal: false, confidence: "high", description: null },
  { id: 2, started_at: at(9, 30), ended_at: at(10, 0), project_path: "/x/beta", is_personal: false, confidence: "high", description: null },
  { id: 3, started_at: at(10, 0), ended_at: at(10, 30), project_path: null, is_personal: false, confidence: "high", description: null }, // unassigned
  { id: 4, started_at: at(10, 30), ended_at: at(11, 0), project_path: null, is_personal: true, confidence: "high", description: null }, // personal
];

describe("buildLanes", () => {
  const { segments, legend } = withLegendHues(buildSegments(blocks, [], window), buildLegend(blocks, []));

  it("orders real project lanes by legend order, with Other last", () => {
    const lanes = buildLanes(segments, legend);
    expect(lanes.map((l) => l.key)).toEqual(["alpha", "beta", "other"]);
  });

  it("each lane holds only its own key's segments", () => {
    const lanes = buildLanes(segments, legend);
    const alpha = lanes.find((l) => l.key === "alpha")!;
    expect(alpha.segments.length).toBe(1);
    expect(alpha.segments.every((s) => s.kind === "block" && s.key === "alpha")).toBe(true);
  });

  it("collects personal + unassigned into one Other lane", () => {
    const lanes = buildLanes(segments, legend);
    const other = lanes.find((l) => l.key === "other")!;
    expect(other.segments.length).toBe(2);
    expect(other.segments.map((s) => (s.kind === "block" ? s.key : s.kind)).sort()).toEqual([
      "personal",
      "unassigned",
    ]);
  });

  it("drops lanes with no segments", () => {
    const onlyAlpha = [blocks[0]];
    const { segments: s2, legend: l2 } = withLegendHues(
      buildSegments(onlyAlpha, [], window),
      buildLegend(onlyAlpha, []),
    );
    expect(buildLanes(s2, l2).map((l) => l.key)).toEqual(["alpha"]);
  });

  it("attaches a lane's activity bands + total seconds from the day summary's activity", () => {
    const activity = [
      { project: "alpha", spans: [{ started_at: at(9, 0), ended_at: at(9, 20) }, { started_at: at(9, 40), ended_at: at(10, 0) }] },
    ];
    const lanes = buildLanes(segments, legend, window, activity);
    const alpha = lanes.find((l) => l.key === "alpha")!;
    expect(alpha.activityBands.length).toBe(2);
    expect(alpha.activitySeconds).toBe(20 * 60 + 20 * 60);
    const beta = lanes.find((l) => l.key === "beta")!;
    expect(beta.activityBands).toEqual([]);
    expect(beta.activitySeconds).toBe(0);
  });

  it("drops activity spans that fall outside the track window", () => {
    const activity = [{ project: "alpha", spans: [{ started_at: at(8, 0), ended_at: at(9, 20) }] }];
    const lanes = buildLanes(segments, legend, window, activity);
    expect(lanes.find((l) => l.key === "alpha")!.activityBands).toEqual([]);
  });

  it("defaults to no activity bands when window/activity aren't passed (back-compat)", () => {
    const lanes = buildLanes(segments, legend);
    expect(lanes.every((l) => l.activityBands.length === 0 && l.activitySeconds === 0)).toBe(true);
  });

  it("puts a plain-path block and a worktree-path block from the same repo in one lane", () => {
    const repoBlocks: StripBlock[] = [
      { id: 10, started_at: at(9, 0), ended_at: at(9, 30), project_path: "/Users/tomas/Desktop/Work/lyfjastofnun", is_personal: false, confidence: "high" },
      { id: 11, started_at: at(9, 30), ended_at: at(10, 0), project_path: "/Users/tomas/Desktop/Work/lyfjastofnun/.claude/worktrees/ci-on-codebuild", is_personal: false, confidence: "high" },
    ];
    const repoWindow = { startMs: new Date(at(9, 0)).getTime(), endMs: new Date(at(10, 0)).getTime() };
    const { segments: rs, legend: rl } = withLegendHues(
      buildSegments(repoBlocks, [], repoWindow),
      buildLegend(repoBlocks, []),
    );
    const lanes = buildLanes(rs, rl);
    expect(lanes.map((l) => l.key)).toEqual(["lyfjastofnun"]);
    expect(lanes[0].segments.length).toBe(2);
  });
});

describe("isFocused", () => {
  // top4 = alpha/beta/gamma/delta, epsilon folds into "+1 more", personal -> Other.
  const legend = compactLegend([
    { key: "alpha", label: "alpha", totalSeconds: 100, hue: 10, slate: false, away: false },
    { key: "beta", label: "beta", totalSeconds: 90, hue: 20, slate: false, away: false },
    { key: "gamma", label: "gamma", totalSeconds: 80, hue: 30, slate: false, away: false },
    { key: "delta", label: "delta", totalSeconds: 70, hue: 40, slate: false, away: false },
    { key: "epsilon", label: "epsilon", totalSeconds: 60, hue: 50, slate: false, away: false },
    { key: "personal", label: "Personal", totalSeconds: 50, hue: null, slate: true, away: false },
  ]);

  it("is true for everything when nothing is focused", () => {
    expect(isFocused("alpha", null, legend)).toBe(true);
    expect(isFocused("epsilon", null, legend)).toBe(true);
  });

  it("is true only for a direct key match", () => {
    expect(isFocused("alpha", "alpha", legend)).toBe(true);
    expect(isFocused("beta", "alpha", legend)).toBe(false);
  });

  it("focuses every folded project when '+N more' is the focus", () => {
    expect(isFocused("epsilon", "more", legend)).toBe(true);
    expect(isFocused("alpha", "more", legend)).toBe(false);
  });

  it("focuses personal + unassigned when 'Other' is the focus", () => {
    expect(isFocused("personal", "other", legend)).toBe(true);
    expect(isFocused("unassigned", "other", legend)).toBe(true);
    expect(isFocused("alpha", "other", legend)).toBe(false);
  });
});
