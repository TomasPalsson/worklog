import "@/app/stats/robot.css";
import { useId } from "react";
import type { PromptStats } from "@/lib/stats_contract";
import { COS30, iso, isoBox, tones } from "./iso";

const nf = new Intl.NumberFormat("en-US");
const W = 336;
const U = 8;
const BOT_H = 136;
const CHAR_W = 0.6; // Geist Mono advance, in em
const COL = 108; // max bubble width; columns sit either side of the plate
const GAP = 6;

export type Mood = "delighted" | "happy" | "neutral" | "wary" | "grumpy";

export const politeness = (p: PromptStats) => p.please + p.thanks + p.sorry;
export const rudeness = (p: PromptStats) => p.swears + p.interrupts / 5;

/** Mood from politeness / (politeness + rudeness). No signal at all -> neutral. */
export function moodOf(p: PromptStats): Mood {
  const pol = politeness(p);
  const total = pol + rudeness(p);
  if (total <= 0) return "neutral";
  const r = pol / total;
  if (r >= 0.85) return "delighted";
  if (r >= 0.6) return "happy";
  if (r >= 0.4) return "neutral";
  if (r >= 0.2) return "wary";
  return "grumpy";
}

export const antennaColor = (m: Mood) =>
  m === "delighted" || m === "happy" ? "var(--sage)" : m === "grumpy" ? "var(--terracotta)" : "var(--amber)";

const LEAD: Record<Mood, string> = {
  delighted: "Impeccably polite",
  happy: "Mostly polite",
  neutral: "Evenly matched",
  wary: "A bit curt",
  grumpy: "Pretty rude",
};

export function headline(p: PromptStats): string {
  if (p.count <= 0) return "Say hello to Claude";
  const n = (v: number, one: string, many: string) => `${nf.format(v)} ${v === 1 ? one : many}`;
  // Always swears vs kind words; interruptions ride along so the mood (which
  // counts 5 cut-offs as one rude word) never reads as contradicting it.
  const cut = p.interrupts > 0 ? ` · cut off ${nf.format(p.interrupts)}×` : "";
  return `${LEAD[moodOf(p)]}: ${n(politeness(p), "kind word", "kind words")} vs ${n(p.swears, "swear", "swears")}${cut}`;
}

export interface BubbleSpec {
  key: string;
  text: string;
  title: string;
  n: number;
  color: string;
}

export function bubbleSpecs(p: PromptStats): BubbleSpec[] {
  const s = (key: string, text: string, title: string, n: number, color: string) => ({ key, text, title, n, color });
  return [
    s("please", `please ×${nf.format(p.please)}`, `said please ${nf.format(p.please)} times`, p.please, "var(--sage)"),
    s("thanks", `thanks ×${nf.format(p.thanks)}`, `said thanks ${nf.format(p.thanks)} times`, p.thanks, "var(--sage)"),
    s("sorry", `sorry ×${nf.format(p.sorry)}`, `said sorry ${nf.format(p.sorry)} times`, p.sorry, "var(--sage)"),
    s("swears", `swore ×${nf.format(p.swears)}`, `swore in ${nf.format(p.swears)} prompts`, p.swears, "var(--terracotta)"),
    s("interrupts", `cut me off ×${nf.format(p.interrupts)}`, `interrupted Claude ${nf.format(p.interrupts)} times`, p.interrupts, "var(--amber)"),
    s("slash", `${nf.format(p.slash_commands)} slash commands`, `${nf.format(p.slash_commands)} slash commands`, p.slash_commands, "var(--slate)"),
    s("questions", `${nf.format(p.questions)} questions`, `${nf.format(p.questions)} prompts ending in ?`, p.questions, "var(--violet)"),
  ].filter((b) => b.n > 0);
}

/** One line up to 14 chars, else split at the last space. */
export function splitLines(t: string): string[] {
  if (t.length <= 14) return [t];
  const i = t.lastIndexOf(" ");
  return i < 0 ? [t] : [t.slice(0, i), t.slice(i + 1)];
}

export interface PlacedBubble extends BubbleSpec {
  lines: string[];
  side: "L" | "R";
  x: number;
  y: number;
  w: number;
  h: number;
  font: number;
}

/**
 * Ring layout: biggest first, alternating left / right, stacked top to bottom
 * (sequential y so two bubbles can never overlap), each side centred on the
 * bot. Size grows with sqrt(count). Returns the region height needed.
 */
export function layoutBubbles(specs: BubbleSpec[]): { placed: PlacedBubble[]; height: number } {
  const sorted = [...specs].sort((a, b) => b.n - a.n);
  const max = sorted[0]?.n ?? 1;
  const sized = sorted.map((b, i) => {
    const lines = splitLines(b.text);
    const longest = Math.max(...lines.map((l) => l.length));
    const r = Math.sqrt(b.n / max);
    const font = Math.min(10 + 3 * r, (COL - 12) / (CHAR_W * longest));
    const w = Math.min(COL, longest * CHAR_W * font + 12);
    const h = lines.length * (font + 3) + 8 + 5 * r;
    return { ...b, lines, side: (i % 2 === 0 ? "L" : "R") as "L" | "R", w, h, font };
  });
  const stackH = (s: "L" | "R") => {
    const m = sized.filter((b) => b.side === s);
    return m.reduce((a, b) => a + b.h, 0) + GAP * Math.max(0, m.length - 1);
  };
  const height = Math.max(BOT_H, stackH("L"), stackH("R"));
  const placed: PlacedBubble[] = [];
  for (const s of ["L", "R"] as const) {
    let y = (height - stackH(s)) / 2;
    for (const b of sized.filter((q) => q.side === s)) {
      placed.push({ ...b, x: s === "L" ? W / 2 - 56 - b.w : W / 2 + 56, y });
      y += b.h + GAP;
    }
  }
  return { placed, height };
}

/** 1/2/5 x 10^k step giving at most ~6 tape intervals; 0 when there is no length. */
export function niceStep(max: number): number {
  if (!(max > 0)) return 0;
  const raw = max / 5;
  const mag = 10 ** Math.floor(Math.log10(raw));
  return ([1, 2, 5, 10].find((m) => m * mag >= raw) ?? 10) * mag;
}

export const tapeShare = (p: PromptStats) =>
  p.longest_chars > 0 ? Math.min(1, Math.max(0, p.avg_chars / p.longest_chars)) : 0;

export function openerSize(n: number, max: number): number {
  return max > 0 ? Math.round(13 + (Math.max(0, n) / max) * 15) : 13;
}

interface FaceSpec {
  r: number;
  arc: boolean;
  smile: number;
  fill: boolean;
  tilt: number;
  brow: 0 | 1 | 2;
}
export const FACES: Record<Mood, FaceSpec> = {
  delighted: { r: 0.5, arc: true, smile: 0.7, fill: true, tilt: 0, brow: 0 },
  happy: { r: 0.5, arc: false, smile: 0.4, fill: false, tilt: 0, brow: 0 },
  neutral: { r: 0.5, arc: false, smile: 0, fill: false, tilt: 0, brow: 0 },
  wary: { r: 0.36, arc: false, smile: -0.15, fill: false, tilt: 0.3, brow: 1 },
  grumpy: { r: 0.42, arc: false, smile: -0.5, fill: false, tilt: 0, brow: 2 },
};

type BoxSpec = { x: number; y: number; z: number; w: number; d: number; h: number };
export const BODY: BoxSpec = { x: 1.8, y: 1.8, z: 0.8, w: 4.4, d: 4.4, h: 3.6 };
export const HEAD: BoxSpec = { x: 1.5, y: 1.5, z: 4.7, w: 5, d: 5, h: 3.8 };
// Left arm sticks out past the body's front (+y) face; the -x side is hidden from the viewer.
export const ARM_L: BoxSpec = { x: 1, y: 5.2, z: 1.4, w: 0.8, d: 1.6, h: 2.4 };
export const ARM_R: BoxSpec = { x: 6.2, y: 3, z: 1.4, w: 0.8, d: 1.6, h: 2.4 };

function Box(props: { x: number; y: number; z: number; w: number; d: number; h: number; c: string }) {
  const f = isoBox(props.x, props.y, props.z, props.w, props.d, props.h, U);
  const t = tones(props.c);
  const st = { stroke: "var(--bg)", strokeWidth: 0.75, strokeLinejoin: "round" as const };
  return (
    <g>
      <polygon points={f.left} fill={t.left} style={st} />
      <polygon points={f.right} fill={t.right} style={st} />
      <polygon points={f.top} fill={t.top} style={st} />
    </g>
  );
}

/** Maps face-local (u along x, v up) grid coords onto the +y face plane at (x, y, z). */
const faceTransform = (x: number, y: number, z: number) => {
  const [ox, oy] = iso(x, y, z, U);
  return `matrix(${COS30 * U} ${0.5 * U} 0 ${-U} ${ox} ${oy})`;
};

function Face({ mood }: { mood: Mood }) {
  const f = FACES[mood];
  const ink = { stroke: "var(--bg)", fill: "none", strokeWidth: 1.8, strokeLinecap: "round" as const, vectorEffect: "non-scaling-stroke" as const };
  const ey = 2.5;
  const m0 = 1.2;
  const mouth = `M1.6 ${m0} Q2.5 ${m0 - 2 * f.smile} 3.4 ${m0 + f.tilt}`;
  const brow = (u: number, left: boolean) => {
    if (f.brow === 0) return null;
    const lift = f.brow === 1 && left ? 0.35 : 0;
    const a = f.brow === 2 ? (left ? 0 : -0.3) : 0;
    const b = f.brow === 2 ? (left ? -0.3 : 0) : 0;
    return <path d={`M${u - 0.55} ${ey + 0.85 + lift + a} L${u + 0.55} ${ey + 0.85 + lift + b}`} style={ink} />;
  };
  return (
    <g transform={faceTransform(1.5, 6.5, 4.7)}>
      <rect x={0.4} y={0.4} width={4.2} height={3.1} fill="var(--fg)" />
      {[1.6, 3.4].map((u, i) =>
        f.arc ? (
          <path key={u} d={`M${u - 0.5} ${ey - 0.15} Q${u} ${ey + 0.7} ${u + 0.5} ${ey - 0.15}`} style={ink} />
        ) : (
          <g key={u}>
            <ellipse cx={u} cy={ey} rx={f.r} ry={f.r} fill="var(--bg)" />
            {brow(u, i === 0)}
          </g>
        ),
      )}
      <path d={f.fill ? `${mouth} Z` : mouth} style={{ ...ink, fill: f.fill ? "var(--bg)" : "none" }} />
    </g>
  );
}

function Bot({ p, mood }: { p: PromptStats; mood: Mood }) {
  const lift = mood === "delighted" ? 0.8 : 0;
  const c = antennaColor(mood);
  const [bx, by] = iso(4, 4, 10.2, U);
  const lamps: [number, number][] = [
    [1.1, p.please],
    [2.2, p.thanks],
    [3.3, p.sorry],
  ];
  return (
    <g className="sx-robot-drop">
      <title>{`Politeness bot, ${mood}. ${nf.format(politeness(p))} kind words, ${nf.format(p.swears)} swears, ${nf.format(p.interrupts)} interruptions.`}</title>
      <Box x={0} y={0} z={-0.5} w={8} d={8} h={0.5} c="var(--bg-sunk)" />
      <Box x={2.4} y={4.6} z={0} w={1.3} d={1.3} h={0.8} c="var(--slate)" />
      <Box x={4.3} y={4.6} z={0} w={1.3} d={1.3} h={0.8} c="var(--slate)" />
      <Box {...BODY} c="var(--slate)" />
      <Box {...ARM_L} z={ARM_L.z + lift} c="var(--slate)" />
      <g transform={faceTransform(1.8, 6.2, 0.8)}>
        {lamps.map(([u, n]) => (
          <ellipse key={u} cx={u} cy={1.9} rx={0.3} ry={0.3} fill={n > 0 ? "var(--sage)" : "var(--border-strong)"} />
        ))}
      </g>
      <Box {...ARM_R} z={ARM_R.z + lift} c="var(--slate)" />
      <Box {...HEAD} c="var(--slate)" />
      <Face mood={mood} />
      <Box x={3.85} y={3.85} z={8.5} w={0.3} d={0.3} h={1.4} c="var(--slate)" />
      <circle cx={bx} cy={by} r={7} fill={c} opacity={0.28} />
      <circle cx={bx} cy={by} r={4} fill={c} />
    </g>
  );
}

function Bubble({ b, i }: { b: PlacedBubble; i: number }) {
  const cy = b.y + b.h / 2;
  const edge = b.side === "L" ? b.x + b.w : b.x;
  const dir = b.side === "L" ? 1 : -1;
  const lh = b.font + 3;
  const t0 = cy - ((b.lines.length - 1) * lh) / 2;
  const fill = `color-mix(in oklch, ${b.color} 22%, var(--bg-raised))`;
  return (
    <g className="sx-robot-pop" style={{ animationDelay: `${300 + i * 80}ms` }}>
      <title>{b.title}</title>
      <polygon points={`${edge},${cy - 4} ${edge + dir * 7},${cy} ${edge},${cy + 4}`} fill={fill} stroke={b.color} strokeWidth={1} />
      <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={8} fill={fill} stroke={b.color} strokeWidth={1} />
      <rect x={edge - 1} y={cy - 3} width={2} height={6} fill={fill} />
      <text x={b.x + b.w / 2} y={t0 + b.font * 0.35} textAnchor="middle" style={{ fontFamily: "var(--font-mono)", fontSize: b.font, fill: "var(--fg)" }}>
        {b.lines.map((l, k) => (
          <tspan key={k} x={b.x + b.w / 2} dy={k === 0 ? 0 : lh}>
            {l}
          </tspan>
        ))}
      </text>
    </g>
  );
}

const TX0 = 44;
const TX1 = W - 12;
function Tape({ p, y }: { p: PromptStats; y: number }) {
  const len = TX1 - TX0;
  const step = niceStep(p.longest_chars);
  const ticks: number[] = [];
  if (step > 0) for (let v = 0; v <= p.longest_chars; v += step) ticks.push(v);
  const ax = TX0 + tapeShare(p) * len;
  const ink = { fontFamily: "var(--font-mono)", fontSize: 10, fill: "var(--fg-muted)" };
  return (
    <g>
      <title>{`Average prompt ${nf.format(Math.round(p.avg_chars))} characters, longest ${nf.format(p.longest_chars)}`}</title>
      <text x={0} y={y + 38} style={ink}>prompt length</text>
      <g className="sx-robot-tape">
        <rect x={TX0 - 4} y={y} width={len + 4} height={22} fill="var(--amber)" />
        <rect x={TX0 - 4} y={y + 22} width={len + 4} height={2} fill={tones("var(--amber)").right} />
        {ticks.map((v) => {
          const x = TX0 + (v / p.longest_chars) * len;
          return <line key={v} x1={x} x2={x} y1={y} y2={y + 8} stroke="var(--fg)" strokeWidth={1} />;
        })}
      </g>
      <rect x={4} y={y - 3} width={TX0 - 4} height={30} rx={6} fill={tones("var(--slate)").left} />
      <circle cx={24} cy={y + 12} r={6} fill="var(--bg)" />
      <g className="art-fade" style={{ animationDelay: "900ms" }}>
        <polygon points={`${ax - 5},${y - 9} ${ax + 5},${y - 9} ${ax},${y - 1}`} fill="var(--fg)" />
        <text x={ax} y={y - 13} textAnchor={ax > TX1 - 60 ? "end" : ax < TX0 + 40 ? "start" : "middle"} style={{ ...ink, fill: "var(--fg)" }}>
          {`avg ${nf.format(Math.round(p.avg_chars))}`}
        </text>
        <text x={TX1} y={y + 38} textAnchor="end" style={ink}>
          {`longest ${nf.format(p.longest_chars)} chars`}
        </text>
      </g>
    </g>
  );
}

export function PoliteBot({ prompt: p }: { prompt: PromptStats }) {
  const uid = useId();
  if (p.count <= 0) {
    return (
      <div className="stats-card sx-robot">
        <h3 className="stats-card-h">How you talk to Claude</h3>
        <svg className="sx-robot-svg" viewBox={`0 0 ${W} 80`} role="img" aria-label="No prompts yet. Nothing built yet.">
          <g transform={`translate(${W / 2} 6)`}>
            <Box x={0} y={0} z={-0.5} w={8} d={8} h={0.5} c="var(--bg-sunk)" />
          </g>
        </svg>
        <p className="sx-robot-empty">nothing built yet. Say hello to Claude.</p>
      </div>
    );
  }
  const mood = moodOf(p);
  const { placed, height } = layoutBubbles(bubbleSpecs(p));
  const dy = (height - BOT_H) / 2;
  const tapeY = height + 40;
  const H = tapeY + 48;
  const max = Math.max(0, ...p.top_openers.map((o) => o.value));
  const aria = `Politeness bot looks ${mood}: ${p.please} please, ${p.thanks} thanks, ${p.sorry} sorry, ${p.swears} swears, ${p.interrupts} interruptions. Average prompt ${Math.round(p.avg_chars)} characters, longest ${p.longest_chars}.`;
  return (
    <div className="stats-card sx-robot">
      <h3 className="stats-card-h">How you talk to Claude</h3>
      <p className="sx-robot-head">{headline(p)}</p>
      <svg id={`${uid}-bot`} className="sx-robot-svg" viewBox={`0 0 ${W} ${H}`} role="img" aria-label={aria}>
        <g transform={`translate(${W / 2} ${62 + dy})`}>
          <Bot p={p} mood={mood} />
        </g>
        {placed.map((b, i) => (
          <Bubble key={b.key} b={b} i={i} />
        ))}
        <Tape p={p} y={tapeY} />
      </svg>
      <p className="sx-robot-legend">Bubble size follows the count. Antenna: green kind, amber neutral, red grumpy. Chest lamps light for please, thanks, sorry.</p>
      {p.top_openers.length > 0 && (
        <ul className="sx-robot-words" aria-label="Most common first words">
          {p.top_openers.map((o) => (
            <li key={o.label} style={{ fontSize: openerSize(o.value, max) }} title={`${o.label}: ${nf.format(o.value)} prompts`}>
              {o.label}
              <small>{nf.format(o.value)}</small>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
