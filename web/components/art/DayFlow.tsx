"use client";

import { useEffect, useId, useState, type ReactNode } from "react";
import { formatDuration } from "@/lib/format";
import { TipLayer } from "@/components/stats/StatTip";
import { tipProps, type Tip } from "@/components/stats/tip";

export type FlowBlock = {
  seconds: number;
  kind: "work" | "personal" | "ignored";
  ticket: string | null;
  sources: { source: string; n: number }[];
};
export type FlowBilling = "billable" | "included" | "not_billable";

export type FlowNode = {
  id: string;
  col: 0 | 1 | 2 | 3;
  label: string;
  seconds: number;
  tone: string; // CSS custom property name
  hatch?: boolean;
};
export type FlowLink = { from: string; to: string; seconds: number };
export type Flow = { nodes: FlowNode[]; links: FlowLink[]; total: number; hasBilling: boolean };

const TOP_TICKETS = 6;
const SEP = "\u0000";

export function sourceLabel(raw: string): string {
  if (raw.startsWith("claude")) return "Claude";
  if (raw.startsWith("git")) return "git";
  if (raw.startsWith("slack")) return "Slack";
  if (raw.startsWith("gcal") || raw === "google_calendar") return "calendar";
  if (raw.startsWith("jira")) return "Jira";
  if (raw.startsWith("browser") || raw.startsWith("web") || raw.startsWith("chrome")) return "web";
  return "other";
}

const KIND: Record<FlowBlock["kind"], { label: string; tone: string }> = {
  work: { label: "work", tone: "--sage" },
  personal: { label: "personal", tone: "--violet" },
  ignored: { label: "ignored", tone: "--fg-subtle" },
};
const BILLING: Record<string, { label: string; tone: string }> = {
  billable: { label: "billable", tone: "--sage" },
  included: { label: "included", tone: "--slate" },
  not_billable: { label: "not billable", tone: "--terracotta" },
  unknown: { label: "unknown", tone: "--amber" },
};

function add(m: Map<string, number>, k: string, v: number) {
  m.set(k, (m.get(k) ?? 0) + v);
}

function buildNodes(nodeSec: Map<string, number>, ranked: [string, number][]): FlowNode[] {
  const nodes: FlowNode[] = [];
  const sources = [...nodeSec.entries()].filter(([id]) => id.startsWith("s:")).sort((a, b) => b[1] - a[1]);
  for (const [id, s] of sources) nodes.push({ id, col: 0, label: id.slice(2), seconds: s, tone: "--slate" });
  for (const k of ["work", "personal", "ignored"] as const) {
    const s = nodeSec.get(`k:${k}`);
    if (s) nodes.push({ id: `k:${k}`, col: 1, label: KIND[k].label, seconds: s, tone: KIND[k].tone });
  }
  for (const [k] of ranked.slice(0, TOP_TICKETS)) {
    nodes.push({ id: `t:${k}`, col: 2, label: k, seconds: nodeSec.get(`t:${k}`) ?? 0, tone: "--slate" });
  }
  const other = nodeSec.get("t:__other__");
  if (other) nodes.push({ id: "t:__other__", col: 2, label: "other", seconds: other, tone: "--fg-subtle" });
  const un = nodeSec.get("t:__unassigned__");
  if (un) nodes.push({ id: "t:__unassigned__", col: 2, label: "unassigned", seconds: un, tone: "--amber", hatch: true });
  for (const c of ["billable", "included", "not_billable", "unknown"]) {
    const s = nodeSec.get(`b:${c}`);
    if (s) nodes.push({ id: `b:${c}`, col: 3, label: BILLING[c].label, seconds: s, tone: BILLING[c].tone, hatch: c === "unknown" });
  }
  return nodes;
}

/** Pure aggregation. `billing` maps ticket key -> class; null = page has no billing data. */
export function buildFlow(blocks: FlowBlock[], billing: Record<string, FlowBilling> | null): Flow {
  const live = blocks.filter((b) => b.seconds > 0);
  const total = live.reduce((a, b) => a + b.seconds, 0);

  const byTicket = new Map<string, number>();
  for (const b of live) if (b.kind === "work" && b.ticket) add(byTicket, b.ticket, b.seconds);
  const ranked = [...byTicket.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  const top = new Set(ranked.slice(0, TOP_TICKETS).map(([k]) => k));
  const ticketNode = (b: FlowBlock) =>
    b.ticket ? (top.has(b.ticket) ? `t:${b.ticket}` : "t:__other__") : "t:__unassigned__";

  const links = new Map<string, number>();
  const link = (f: string, t: string, v: number) => add(links, f + SEP + t, v);
  for (const b of live) {
    // ponytail: a block's seconds are split across its sources by event count; no per-event timing on the page.
    const n = b.sources.reduce((a, s) => a + Math.max(0, s.n), 0);
    if (n === 0) link("s:other", `k:${b.kind}`, b.seconds);
    else
      for (const s of b.sources) {
        if (s.n > 0) link(`s:${sourceLabel(s.source)}`, `k:${b.kind}`, (b.seconds * s.n) / n);
      }
    if (b.kind === "work") {
      const t = ticketNode(b);
      link("k:work", t, b.seconds);
      if (billing) link(t, `b:${(b.ticket && billing[b.ticket]) || "unknown"}`, b.seconds);
    }
  }

  // node seconds = everything flowing in (sources: everything flowing out)
  const nodeSec = new Map<string, number>();
  for (const [k, v] of links) {
    const [f, t] = k.split(SEP);
    if (f.startsWith("s:")) add(nodeSec, f, v);
    add(nodeSec, t, v);
  }

  const nodes = buildNodes(nodeSec, ranked);

  return {
    nodes,
    links: [...links.entries()].map(([k, seconds]) => {
      const [from, to] = k.split(SEP);
      return { from, to, seconds };
    }),
    total,
    hasBilling: billing !== null && nodes.some((n) => n.col === 3),
  };
}

export type Placed = FlowNode & { x: number; y: number; h: number };
export type Ribbon = { key: string; d: string; tone: string; from: string; to: string };

const NODE_W = 8;
const GAP = 12;
const MIN_H = 3;
const CHAR_W = 7; // 11px mono label + letter-spacing, rounded up

/** Pure layout: heights proportional to seconds with one shared scale. */
export function layoutFlow(flow: Flow, width: number, withSources: boolean, height = 170) {
  const cols = ([0, 1, 2, 3] as const).filter(
    (c) => (c !== 0 || withSources) && (c !== 3 || flow.hasBilling) && flow.nodes.some((n) => n.col === c),
  );
  const byCol = cols.map((c) => flow.nodes.filter((n) => n.col === c));
  let scale = Infinity;
  for (const ns of byCol) {
    const sum = ns.reduce((a, n) => a + n.seconds, 0);
    if (sum > 0) scale = Math.min(scale, Math.max(0, height - GAP * (ns.length - 1)) / sum);
  }
  if (!Number.isFinite(scale)) scale = 0;
  const lastCol = byCol[byCol.length - 1] ?? [];
  const room = Math.ceil(
    Math.max(0, ...lastCol.map((n) => `${n.label} ${formatDuration(n.seconds)}`.length)) * CHAR_W + 8,
  );
  const span = Math.max(0, width - NODE_W - 4 - room - 2); // right-hand room for the last labels
  const placed = new Map<string, Placed>();
  byCol.forEach((ns, ci) => {
    const x = 2 + (cols.length === 1 ? 0 : (span * ci) / (cols.length - 1));
    let y = 0;
    for (const n of ns) {
      const h = Math.max(MIN_H, n.seconds * scale);
      placed.set(n.id, { ...n, x, y, h });
      y += h + GAP;
    }
  });
  const ids = [...placed.keys()];
  const outCur = new Map<string, number>();
  const inCur = new Map<string, number>();
  const ribbons: Ribbon[] = [];
  const ls = flow.links
    .filter((l) => placed.has(l.from) && placed.has(l.to))
    .sort((a, b) => ids.indexOf(a.to) - ids.indexOf(b.to) || ids.indexOf(a.from) - ids.indexOf(b.from));
  const f = (n: number) => n.toFixed(2);
  for (const l of ls) {
    const a = placed.get(l.from)!;
    const b = placed.get(l.to)!;
    const ah = (l.seconds / a.seconds) * a.h;
    const bh = (l.seconds / b.seconds) * b.h;
    const ay = a.y + (outCur.get(a.id) ?? 0);
    const by = b.y + (inCur.get(b.id) ?? 0);
    outCur.set(a.id, (outCur.get(a.id) ?? 0) + ah);
    inCur.set(b.id, (inCur.get(b.id) ?? 0) + bh);
    const x0 = a.x + NODE_W;
    const x1 = b.x;
    const m = (x0 + x1) / 2;
    const d =
      `M${f(x0)},${f(ay)} C${f(m)},${f(ay)} ${f(m)},${f(by)} ${f(x1)},${f(by)} ` +
      `L${f(x1)},${f(by + bh)} C${f(m)},${f(by + bh)} ${f(m)},${f(ay + ah)} ${f(x0)},${f(ay + ah)} Z`;
    ribbons.push({ key: `${l.from}>${l.to}`, d, tone: a.tone, from: a.id, to: b.id });
  }
  const bottom = Math.max(0, ...[...placed.values()].map((n) => n.y + n.h));
  return { nodes: [...placed.values()], ribbons, height: Math.max(40, bottom) + 4, width };
}

/** Words for the biggest flows, for the aria-label. */
export function describeFlow(flow: Flow, withSources: boolean): string {
  const name = new Map(flow.nodes.map((n) => [n.id, n.label]));
  const big = flow.links
    .filter((l) => withSources || !l.from.startsWith("s:"))
    .sort((a, b) => b.seconds - a.seconds)
    .slice(0, 4)
    .map((l) => `${name.get(l.from)} to ${name.get(l.to)} ${formatDuration(l.seconds)}`);
  return `Day flow, ${formatDuration(flow.total)} in total. Biggest flows: ${big.join("; ")}.`;
}

const pct = (a: number, b: number) => (b > 0 ? `${Math.round((a / b) * 100)}%` : "0%");
const COL_NAME = ["source", "kind", "ticket", "billing"] as const;

/** Top-3 flows touching a node on one side, as label/value rows. */
function topFlows(flow: Flow, id: string, side: "in" | "out", withSources: boolean): [string, string][] {
  const name = new Map(flow.nodes.map((n) => [n.id, n.label]));
  return flow.links
    .filter((l) => (side === "in" ? l.to === id : l.from === id) && (withSources || !l.from.startsWith("s:")))
    .sort((a, b) => b.seconds - a.seconds)
    .slice(0, 3)
    .map((l) => [`${side === "in" ? "from" : "to"} ${name.get(side === "in" ? l.from : l.to)}`, formatDuration(l.seconds)]);
}

/** Tip for a node: total, share of its column, top 3 flows each way. */
export function nodeTip(flow: Flow, n: FlowNode, withSources: boolean): Tip {
  const colTotal = flow.nodes.filter((x) => x.col === n.col).reduce((a, x) => a + x.seconds, 0);
  const share = pct(n.seconds, colTotal);
  const rows: [string, string][] = [
    ["Total", formatDuration(n.seconds)],
    [`Share of ${COL_NAME[n.col]} column`, share],
    ...topFlows(flow, n.id, "in", withSources),
    ...topFlows(flow, n.id, "out", withSources),
  ];
  const biggest = [...flow.nodes].filter((x) => x.col === n.col).sort((a, b) => b.seconds - a.seconds)[0];
  return {
    title: n.label,
    sub: `${COL_NAME[n.col]} · ${formatDuration(n.seconds)} of ${formatDuration(colTotal)}`,
    rows,
    bar: { value: n.seconds, max: colTotal, label: `${share} of the ${COL_NAME[n.col]} column` },
    note: biggest && biggest.id === n.id && flow.nodes.filter((x) => x.col === n.col).length > 1
      ? `The biggest ${COL_NAME[n.col]} of the day.`
      : undefined,
    accent: `var(${n.tone})`,
  };
}

/** Tip for a ribbon: from to to, duration, share of both ends. */
export function ribbonTip(flow: Flow, r: Ribbon): Tip {
  const name = new Map(flow.nodes.map((n) => [n.id, n]));
  const a = name.get(r.from);
  const b = name.get(r.to);
  const secs = flow.links.find((l) => l.from === r.from && l.to === r.to)?.seconds ?? 0;
  return {
    title: `${a?.label ?? r.from} \u2192 ${b?.label ?? r.to}`,
    sub: formatDuration(secs),
    rows: [
      ["Duration", formatDuration(secs)],
      [`Share of ${a?.label ?? "source"}`, pct(secs, a?.seconds ?? 0)],
      [`Share of ${b?.label ?? "target"}`, pct(secs, b?.seconds ?? 0)],
      ["Share of all tracked time", pct(secs, flow.total)],
    ],
    bar: { value: secs, max: a?.seconds ?? 0, label: `${pct(secs, a?.seconds ?? 0)} of ${a?.label ?? "source"}` },
    accent: `var(${r.tone})`,
  };
}

const ribbonAria = (flow: Flow, r: Ribbon) => {
  const t = ribbonTip(flow, r);
  return `${t.title}, ${t.sub}`;
};

function FlowSvg({ flow, width, height, withSources, cls }: { flow: Flow; width: number; height: number; withSources: boolean; cls: string }) {
  const uid = useId();
  const clip = `${uid}-clip`;
  const hatch = `${uid}-hatch`;
  const L = layoutFlow(flow, width, withSources, height);
  // Phone layout: inner columns are too close for "GENAI-1906 1h 57m", so
  // only the last column keeps its durations (the aria-label has them all).
  const lastCol = Math.max(...L.nodes.map((n) => n.col));
  const text = (n: Placed) => (withSources || n.col === lastCol ? `${n.label} ${formatDuration(n.seconds)}` : n.label);
  return (
    <svg
      className={`art-dayflow-svg ${cls}`}
      viewBox={`0 0 ${L.width} ${L.height}`}
      role="img"
      aria-label={describeFlow(flow, withSources)}
    >
      <defs>
        <clipPath id={clip}>
          <rect className="art-dayflow-reveal" x="0" y="0" width={L.width} height={L.height} />
        </clipPath>
        <pattern id={hatch} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(135)">
          <line x1="0" y1="0" x2="0" y2="6" stroke="var(--amber)" strokeWidth="3" />
        </pattern>
      </defs>
      <g className="art-dayflow-ribbons" clipPath={`url(#${clip})`}>
        {L.ribbons.map((r) => (
          <path
            key={r.key}
            className="art-dayflow-rib"
            d={r.d}
            style={{ fill: `var(${r.tone})` }}
            tabIndex={0}
            aria-label={ribbonAria(flow, r)}
            {...tipProps(ribbonTip(flow, r))}
          />
        ))}
      </g>
      {L.nodes.map((n) => (
        <g
          key={n.id}
          className="art-dayflow-node"
          tabIndex={0}
          aria-label={`${n.label}, ${formatDuration(n.seconds)}`}
          {...tipProps(nodeTip(flow, n, withSources))}
        >
          <rect x={n.x} y={n.y} width={NODE_W} height={n.h} rx="3" style={{ fill: `var(${n.tone})` }} />
          {n.hatch && <rect x={n.x} y={n.y} width={NODE_W} height={n.h} rx="3" fill={`url(#${hatch})`} />}
          <text className="art-label art-dayflow-text" x={n.x + NODE_W + 4} y={n.y + n.h / 2 + 3.5}>
            {text(n)}
          </text>
        </g>
      ))}
    </svg>
  );
}

const KEY = "worklog.dayflow.open";

export function DayFlow({
  blocks,
  billing,
  title,
  defaultOpen = false,
}: {
  blocks: FlowBlock[];
  billing: Record<string, FlowBilling> | null;
  title?: ReactNode;
  /** Start open and neither read nor write the remembered preference. */
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  useEffect(() => {
    if (defaultOpen) return;
    try {
      if (localStorage.getItem(KEY) === "1") setOpen(true);
    } catch {
      /* storage unavailable: stay collapsed */
    }
  }, [defaultOpen]);
  const flow = buildFlow(blocks, billing);
  if (flow.total <= 0) return null;
  // Same total as the page header: ignored time is drawn, not counted.
  const kept = blocks.reduce((a, b) => (b.kind === "ignored" ? a : a + b.seconds), 0);
  return (
    <TipLayer>
    <details
      className="art-dayflow"
      open={open}
      onToggle={(e) => {
        const o = e.currentTarget.open;
        setOpen(o);
        if (defaultOpen) return;
        try {
          localStorage.setItem(KEY, o ? "1" : "0");
        } catch {
          /* storage unavailable: not remembered */
        }
      }}
    >
      <summary className="art-dayflow-summary">
        <span className="art-dayflow-chev" aria-hidden="true">▸</span>
        <span>
          {title ?? (
            <>
              Where the day&rsquo;s <span className="art-label art-dayflow-total">{formatDuration(kept)}</span> went
            </>
          )}
        </span>
      </summary>
      {open && (
        <div className="art-dayflow-body">
          <FlowSvg flow={flow} width={1100} height={220} withSources cls="art-dayflow-wide" />
          <FlowSvg flow={flow} width={360} height={170} withSources={false} cls="art-dayflow-narrow" />
        </div>
      )}
    </details>
    </TipLayer>
  );
}
