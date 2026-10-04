// Pure geometry for the week timeline: visible hour range, hour height,
// block position and overlap lanes. Times are local, like formatRange.

export interface Span {
  started_at: string;
  ended_at: string;
  is_personal: boolean;
}

const MIN_START = 8;
const MIN_END = 18;

/** Local minutes since midnight of an ISO instant. */
export const localMinutes = (iso: string): number => {
  const d = new Date(iso);
  return d.getHours() * 60 + d.getMinutes();
};

const lengthMin = (b: Span) => Math.max(0, (Date.parse(b.ended_at) - Date.parse(b.started_at)) / 60000);

/** Whole-hour range covering every work block, never narrower than 08:00–18:00. */
export function timelineRange(blocks: Span[]): { startHour: number; endHour: number } {
  let startHour = MIN_START;
  let endHour = MIN_END;
  for (const b of blocks) {
    if (b.is_personal) continue;
    const s = localMinutes(b.started_at);
    startHour = Math.min(startHour, Math.floor(s / 60));
    endHour = Math.max(endHour, Math.ceil((s + lengthMin(b)) / 60));
  }
  return { startHour, endHour: Math.min(endHour, 24) };
}

/** `clamp(36px, 640px / hours, 56px)`. */
export const hourHeight = (hours: number): number => Math.min(56, Math.max(36, 640 / hours));

/** Minutes from the range start and length, clipped to the range; null when fully outside. */
export function blockSpan(
  b: Span,
  range: { startHour: number; endHour: number },
): { top: number; length: number } | null {
  const lo = range.startHour * 60;
  const hi = range.endHour * 60;
  const s = localMinutes(b.started_at);
  const start = Math.max(s, lo);
  const end = Math.min(s + lengthMin(b), hi);
  return end > start ? { top: start - lo, length: end - start } : null;
}

/**
 * Greedy lane assignment. Items sort by start; each takes the first lane
 * whose last end is at or before its start. `lanes` is the lane count of the
 * item's overlap cluster, so width = 1 / lanes. Returned in input order.
 */
export function assignLanes(items: { start: number; end: number }[]): { lane: number; lanes: number }[] {
  const order = items.map((_, i) => i).sort((a, b) => items[a].start - items[b].start || items[a].end - items[b].end);
  const out = new Array<{ lane: number; lanes: number }>(items.length);
  let laneEnds: number[] = [];
  let cluster: number[] = [];
  const close = () => {
    for (const i of cluster) out[i].lanes = laneEnds.length;
    laneEnds = [];
    cluster = [];
  };
  let clusterEnd = -Infinity;
  for (const i of order) {
    const { start, end } = items[i];
    if (start >= clusterEnd) close();
    let lane = laneEnds.findIndex((e) => e <= start);
    if (lane < 0) lane = laneEnds.length;
    laneEnds[lane] = end;
    clusterEnd = cluster.length === 0 ? end : Math.max(clusterEnd, end);
    out[i] = { lane, lanes: 1 };
    cluster.push(i);
  }
  close();
  return out;
}

export type RunStatus = "synced" | "dirty" | "ticket" | "none";

interface RunBlock extends Span {
  jira_issue: string | null;
  tempo_worklog_id: string | null;
  dirty: boolean;
  duration_seconds: number;
  description: string | null;
}

export interface Run<B extends RunBlock = RunBlock> extends Span {
  jira_issue: string | null;
  status: RunStatus;
  workSeconds: number;
  blocks: B[];
  description: string | null;
  count: number;
}

export function blockStatus(b: RunBlock): RunStatus {
  if (b.tempo_worklog_id) return b.dirty ? "dirty" : "synced";
  return b.jira_issue ? "ticket" : "none";
}

const mergeKey = (b: RunBlock) => (b.is_personal ? "personal" : `${blockStatus(b)}|${b.jira_issue ?? "none"}`);

/**
 * Collapse one day's blocks into runs: consecutive blocks with the same merge
 * key whose start is within `gapMinutes` of the run's end (overlap counts).
 * `workSeconds` sums member durations, not the span.
 */
export function mergeRuns<B extends RunBlock>(blocks: B[], gapMinutes = 10): Run<B>[] {
  const sorted = [...blocks].sort((a, b) => Date.parse(a.started_at) - Date.parse(b.started_at));
  const runs: Run<B>[] = [];
  const keys: string[] = [];
  for (const b of sorted) {
    const k = mergeKey(b);
    const r = runs[runs.length - 1];
    if (r && keys[keys.length - 1] === k && Date.parse(b.started_at) <= Date.parse(r.ended_at) + gapMinutes * 60000) {
      r.blocks.push(b);
      if (Date.parse(b.ended_at) > Date.parse(r.ended_at)) r.ended_at = b.ended_at;
      r.workSeconds += b.duration_seconds;
      r.count++;
      continue;
    }
    keys.push(k);
    runs.push({
      started_at: b.started_at,
      ended_at: b.ended_at,
      is_personal: b.is_personal,
      jira_issue: b.jira_issue,
      status: blockStatus(b),
      workSeconds: b.duration_seconds,
      blocks: [b],
      description: null,
      count: 1,
    });
  }
  for (const r of runs) {
    const longest = r.blocks.reduce((a, b) => (lengthMin(b) > lengthMin(a) ? b : a));
    r.description = longest.description || null;
  }
  return runs;
}

/** Distinct non-empty member descriptions with summed seconds, longest first. */
export function runActivities(run: Run): { description: string; seconds: number }[] {
  const sums = new Map<string, number>();
  for (const b of run.blocks) {
    if (b.description) sums.set(b.description, (sums.get(b.description) ?? 0) + b.duration_seconds);
  }
  return [...sums].map(([description, seconds]) => ({ description, seconds })).sort((a, b) => b.seconds - a.seconds);
}
