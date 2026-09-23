// Pure maths for <DayStrip>: the horizontal timeline of a day's blocks and
// gaps. Kept dependency-free and framework-free so the layout/hashing logic
// is unit-testable without a render harness.

import { isLunchGap } from "./format";

/** Minimal block shape the strip needs — decoupled from the full `Block`
 * type so this module (and its tests) don't have to construct one. */
export interface StripBlock {
  id: number;
  started_at: string; // ISO-8601
  ended_at: string; // ISO-8601
  project_path: string | null;
  is_personal: boolean;
  confidence: "high" | "medium" | "low";
  /** Optional so existing StripBlock literals in tests don't all need
   * updating — buildSegments treats a missing value as `null`. */
  description?: string | null;
}

export interface StripGap {
  started_at: string; // ISO-8601
  ended_at: string; // ISO-8601
  minutes: number;
}

export interface TrackWindow {
  startMs: number;
  endMs: number;
}

/** The track spans the first block's start floored to the hour to the last
 * block's end ceiled to the hour, local time. `null` when there are no
 * blocks — the strip itself renders nothing in that case. */
export function computeTrackWindow(blocks: StripBlock[]): TrackWindow | null {
  if (blocks.length === 0) return null;
  let minStart = Infinity;
  let maxEnd = -Infinity;
  for (const b of blocks) {
    const s = new Date(b.started_at).getTime();
    const e = new Date(b.ended_at).getTime();
    if (s < minStart) minStart = s;
    if (e > maxEnd) maxEnd = e;
  }
  return {
    startMs: floorToHour(minStart),
    endMs: ceilToHour(maxEnd),
  };
}

function floorToHour(ms: number): number {
  const d = new Date(ms);
  d.setMinutes(0, 0, 0);
  return d.getTime();
}

function ceilToHour(ms: number): number {
  const d = new Date(ms);
  if (d.getMinutes() === 0 && d.getSeconds() === 0 && d.getMilliseconds() === 0) {
    return d.getTime();
  }
  d.setHours(d.getHours() + 1, 0, 0, 0);
  return d.getTime();
}

export interface HourTick {
  ms: number;
  pct: number;
  label: string;
}

/** Hour-tick positions under the track. Every hour for spans of ten hours
 * or less, every two hours for longer days — a 14h day at 1h ticks is just
 * noise. */
export function hourTicks(window: TrackWindow): HourTick[] {
  const totalMs = window.endMs - window.startMs;
  if (totalMs <= 0) return [];
  const totalHours = totalMs / 3_600_000;
  const stepHours = totalHours <= 10 ? 1 : 2;
  const stepMs = stepHours * 3_600_000;
  const ticks: HourTick[] = [];
  for (let ms = window.startMs; ms <= window.endMs; ms += stepMs) {
    const d = new Date(ms);
    ticks.push({
      ms,
      pct: ((ms - window.startMs) / totalMs) * 100,
      label: `${String(d.getHours()).padStart(2, "0")}:00`,
    });
  }
  return ticks;
}

/** Grouping key for colour + legend: the last path segment of a work
 * block's project_path, "personal" for personal blocks (regardless of
 * path), or "unassigned" when there's no path at all. */
export function projectKey(block: Pick<StripBlock, "project_path" | "is_personal">): string {
  if (block.is_personal) return "personal";
  if (!block.project_path) return "unassigned";
  const trimmed = block.project_path.replace(/\/+$/, "");
  const segments = trimmed.split("/").filter(Boolean);
  return segments[segments.length - 1] ?? "unassigned";
}

/** "personal"/"unassigned" render --slate rather than a hashed hue — they
 * aren't a real project, so a stable colour would be misleading. */
export function isSlateKey(key: string): boolean {
  return key === "personal" || key === "unassigned";
}

export function displayLabel(key: string): string {
  if (key === "unassigned") return "Unassigned";
  if (key === "personal") return "Personal";
  return key;
}

/**
 * Deterministic hash of a project key to a hue 0–360. Same key always
 * gives the same hue; different keys almost always differ. A plain
 * string hash (à la `String.hashCode`) is plenty — this only has to be
 * stable and visually distinct, not cryptographically sound.
 */
export function projectHue(key: string): number {
  let h = 0;
  for (let i = 0; i < key.length; i++) {
    h = (h * 31 + key.charCodeAt(i)) | 0;
  }
  return Math.abs(h) % 360;
}

export function confidenceOpacity(confidence: StripBlock["confidence"]): number {
  switch (confidence) {
    case "high":
      return 1;
    case "medium":
      return 0.72;
    case "low":
      return 0.45;
  }
}

export function clock(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function minutesBetween(startMs: number, endMs: number): number {
  return Math.round((endMs - startMs) / 60_000);
}

export type TrackSegment =
  | {
      kind: "block";
      blockId: number;
      startMs: number;
      endMs: number;
      leftPct: number;
      widthPct: number;
      key: string;
      label: string;
      hue: number;
      slate: boolean;
      opacity: number;
      ariaLabel: string;
      /** First block's description — merging never overwrites it. */
      description: string | null;
      /** How many original blocks are folded into this segment (>1 once
       * mergeAdjacentBlocks has joined neighbours). */
      mergedCount: number;
    }
  | {
      kind: "gap";
      startMs: number;
      endMs: number;
      leftPct: number;
      widthPct: number;
      minutes: number;
      isLunch: boolean;
      label: string;
      showLabel: boolean;
      /** Room for the bare label ("Lunch") but not the minutes. */
      showShortLabel: boolean;
      ariaLabel: string;
    };

/** Gap centre labels only show once the segment is wide enough to hold
 * them — below this the text would overflow its own segment. */
const GAP_LABEL_MIN_WIDTH_PCT = 10;
const GAP_SHORT_LABEL_MIN_WIDTH_PCT = 4;

/** Lay out every block and gap as a percentage-positioned segment of the
 * track, in chronological order. */
export function buildSegments(
  blocks: StripBlock[],
  gaps: StripGap[],
  window: TrackWindow,
): TrackSegment[] {
  const totalMs = window.endMs - window.startMs;
  if (totalMs <= 0) return [];

  const pct = (ms: number) => ((ms - window.startMs) / totalMs) * 100;

  const blockSegments: TrackSegment[] = blocks.map((b) => {
    const startMs = new Date(b.started_at).getTime();
    const endMs = new Date(b.ended_at).getTime();
    const key = projectKey(b);
    const slate = isSlateKey(key);
    const label = displayLabel(key);
    const minutes = minutesBetween(startMs, endMs);
    return {
      kind: "block",
      blockId: b.id,
      startMs,
      endMs,
      leftPct: pct(startMs),
      widthPct: pct(endMs) - pct(startMs),
      key,
      label,
      hue: projectHue(key),
      slate,
      opacity: confidenceOpacity(b.confidence),
      ariaLabel: `${label} ${clock(startMs)}–${clock(endMs)}, ${minutes} min, ${b.confidence} confidence`,
      description: b.description ?? null,
      mergedCount: 1,
    };
  });

  const gapSegments: TrackSegment[] = gaps.map((g) => {
    const startMs = new Date(g.started_at).getTime();
    const endMs = new Date(g.ended_at).getTime();
    const isLunch = isLunchGap(g);
    const label = isLunch ? "Lunch" : "Away";
    const widthPct = pct(endMs) - pct(startMs);
    return {
      kind: "gap",
      startMs,
      endMs,
      leftPct: pct(startMs),
      widthPct,
      minutes: g.minutes,
      isLunch,
      label,
      showLabel: widthPct >= GAP_LABEL_MIN_WIDTH_PCT,
      showShortLabel: widthPct >= GAP_SHORT_LABEL_MIN_WIDTH_PCT,
      ariaLabel: `${label} ${clock(startMs)}–${clock(endMs)}, ${g.minutes} min`,
    };
  });

  return [...blockSegments, ...gapSegments].sort((a, b) => a.startMs - b.startMs);
}

export interface LegendEntry {
  key: string;
  label: string;
  totalSeconds: number;
  hue: number | null;
  slate: boolean;
  away: boolean;
  /** Hover text, e.g. the projects folded into "+2 more". */
  title?: string;
}

/** Legend rows: one per project (by total desc), then "Away" last with the
 * summed gap time — Away isn't sorted in among the projects, it's always
 * the closing entry. */
export function buildLegend(blocks: StripBlock[], gaps: StripGap[]): LegendEntry[] {
  const totals = new Map<string, number>();
  for (const b of blocks) {
    const key = projectKey(b);
    const seconds =
      (new Date(b.ended_at).getTime() - new Date(b.started_at).getTime()) / 1000;
    totals.set(key, (totals.get(key) ?? 0) + seconds);
  }

  const entries: LegendEntry[] = Array.from(totals.entries()).map(([key, totalSeconds]) => ({
    key,
    label: displayLabel(key),
    totalSeconds,
    hue: isSlateKey(key) ? null : projectHue(key),
    slate: isSlateKey(key),
    away: false,
  }));
  entries.sort((a, b) => b.totalSeconds - a.totalSeconds);

  if (gaps.length > 0) {
    const awaySeconds = gaps.reduce((acc, g) => acc + g.minutes * 60, 0);
    entries.push({
      key: "away",
      label: "Away",
      totalSeconds: awaySeconds,
      hue: null,
      slate: true,
      away: true,
    });
  }

  return entries;
}

/** Golden-angle spacing keeps any two of the day's projects far apart on the
 * colour wheel; a name hash alone can land two busy projects on one teal. */
const GOLDEN_ANGLE = 137.508;
const HUE_OFFSET = 40;

/** Re-hue the legend by rank (busiest project first) and colour each block
 * segment with its project's legend hue, so strip and legend always agree. */
export function withLegendHues(
  segments: TrackSegment[],
  legend: LegendEntry[],
): { segments: TrackSegment[]; legend: LegendEntry[] } {
  let rank = 0;
  const hueByKey = new Map<string, number>();
  const hued = legend.map((e) => {
    if (e.slate || e.away) return e;
    const hue = Math.round((HUE_OFFSET + rank++ * GOLDEN_ANGLE) % 360);
    hueByKey.set(e.key, hue);
    return { ...e, hue };
  });
  const recoloured = segments.map((s) =>
    s.kind === "block" && hueByKey.has(s.key) ? { ...s, hue: hueByKey.get(s.key) as number } : s,
  );
  return { segments: recoloured, legend: hued };
}

/** Same-project blocks closer than this read as one stretch of work. */
const MERGE_GAP_MS = 10 * 60 * 1000;

/** Join consecutive same-project block segments (no gap segment between,
 * under MERGE_GAP_MS apart) so the strip shows stretches, not fragments. */
export function mergeAdjacentBlocks(segments: TrackSegment[]): TrackSegment[] {
  const out: TrackSegment[] = [];
  for (const seg of segments) {
    const prev = out[out.length - 1];
    if (
      seg.kind === "block" &&
      prev?.kind === "block" &&
      prev.key === seg.key &&
      seg.startMs - prev.endMs < MERGE_GAP_MS
    ) {
      const endMs = Math.max(prev.endMs, seg.endMs);
      const rightPct = Math.max(prev.leftPct + prev.widthPct, seg.leftPct + seg.widthPct);
      out[out.length - 1] = {
        ...prev,
        endMs,
        widthPct: rightPct - prev.leftPct,
        opacity: Math.max(prev.opacity, seg.opacity),
        ariaLabel: `${prev.label} ${clock(prev.startMs)}–${clock(endMs)}, ${minutesBetween(prev.startMs, endMs)} min`,
        // Keep the first block's description; just tally how many blocks
        // are now folded into this stretch for the tooltip's "and N more".
        mergedCount: prev.mergedCount + seg.mergedCount,
      };
      continue;
    }
    out.push(seg);
  }
  return out;
}

const LEGEND_TOP_PROJECTS = 4;

/** Top projects, then "+N more", then personal/unassigned as "Other", then Away. */
export function compactLegend(legend: LegendEntry[]): LegendEntry[] {
  const projects = legend.filter((e) => !e.slate && !e.away);
  const other = legend.filter((e) => e.slate && !e.away);
  const away = legend.filter((e) => e.away);
  const out = projects.slice(0, LEGEND_TOP_PROJECTS);
  const rest = projects.slice(LEGEND_TOP_PROJECTS);
  const sum = (es: LegendEntry[]) => es.reduce((a, e) => a + e.totalSeconds, 0);
  if (rest.length > 0)
    out.push({ key: "more", label: `+${rest.length} more`, totalSeconds: sum(rest), hue: null, slate: true, away: false, title: rest.map((e) => e.label).join(", ") });
  if (other.length > 0)
    out.push({ key: "other", label: "Other", totalSeconds: sum(other), hue: null, slate: true, away: false, title: other.map((e) => e.label).join(", ") });
  return [...out, ...away];
}

// The "lanes" expanded view + legend-focus interaction live in
// ./dayStripLanes — see that module for `Lane`, `buildLanes`, `isFocused`.
