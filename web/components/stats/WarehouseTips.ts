import type { Ranked } from "@/lib/stats_contract";
import type { Tip } from "./tip";
import { fmt, shortTool, stackCrates } from "./ToolWarehouse";

/** Whole-number percent; "<1%" for a nonzero sliver. */
const pctOf = (v: number, of: number) => (of > 0 ? (v > 0 && v / of < 0.005 ? "<1%" : `${Math.round((v / of) * 100)}%`) : "0%");
const perDay = (v: number, days?: number) => (days && days > 0 ? (v / days).toFixed(1) : null);

/** Sorted-desc, positive-only copy of the tool list: the ranking every tip reads. */
export const ranked = (tools: Ranked[]): Ranked[] => tools.filter((t) => t.value > 0).sort((a, b) => b.value - a.value);

const plural = (n: number, w: string) => `${fmt(n)} ${w}${n === 1 ? "" : "s"}`;

/** Tip for one tool's pallet: `list` is `ranked(tools)`, `all` the total tool calls. */
export function palletTip(tool: Ranked, list: Ranked[], all: number, unit: number, days?: number, accent?: string): Tip {
  const idx = list.findIndex((t) => t.label === tool.label);
  const rank = idx < 0 ? list.length + 1 : idx + 1;
  const name = shortTool(tool.label);
  const next = list[rank]; // the tool ranked just below
  const top = list[0] ?? tool;
  const below = list.slice(rank);
  const belowSum = below.reduce((s, t) => s + t.value, 0);
  const crates = stackCrates(tool.value, unit).length;
  const day = perDay(tool.value, days);
  const rows: [string, string][] = [
    ["Calls", fmt(tool.value)],
    ["Share of all calls", pctOf(tool.value, all)],
    ["Crates", `${fmt(crates)} (1 crate = ${fmt(unit)})`],
  ];
  if (day) rows.push(["Per worked day", `${day} calls`]);
  const tied = !!next && tool.value === next.value;
  rows.push(["Gap to next", next ? (tied ? `tied with ${shortTool(next.label)}` : `${fmt(tool.value - next.value)} ahead of ${shortTool(next.label)}`) : "last of the shown tools"]);
  let note: string;
  if (below.length === 0) note = rank === 1 ? `${name} is the only tool in the warehouse.` : `${name} brings up the rear.`;
  else if (tool.value > belowSum) {
    note = `${name} ${rank === 1 ? "alone outruns the next" : "outruns the"} ${plural(below.length, "tool")}${rank === 1 ? "" : " below it"} combined (${(tool.value / belowSum).toFixed(1)}×).`;
  } else if (tied && next) note = `${name} is tied with ${shortTool(next.label)}.`;
  else if (next) note = `Only ${fmt(tool.value - next.value)} calls separate ${name} from ${shortTool(next.label)}.`;
  else note = `${name} brings up the rear.`;
  return {
    title: name,
    sub: `#${rank} of ${plural(list.length, "tool")}`,
    rows,
    bar: { value: tool.value, max: top.value, label: rank === 1 ? "the busiest tool" : `vs ${shortTool(top.label)}` },
    note,
    accent,
  };
}

/** The forklift hauls the top tool. */
export function forkliftTip(top: Ranked, all: number, unit: number, days?: number): Tip {
  const name = shortTool(top.label);
  const day = perDay(top.value, days);
  const rows: [string, string][] = [
    ["Load", `${name}, ${fmt(top.value)} calls`],
    ["Share of all calls", pctOf(top.value, all)],
    ["Crates", `${fmt(stackCrates(top.value, unit).length)} (1 crate = ${fmt(unit)})`],
  ];
  if (day) rows.push(["Per worked day", `${day} calls`]);
  return {
    title: `Forklift: hauling ${name}`,
    sub: "the top tool, always on the move",
    rows,
    bar: { value: top.value, max: all, label: "of all tool calls" },
    note: `${pctOf(top.value, all)} of every tool call is ${name}, so the forklift never clocks off.`,
    accent: "var(--amber)",
  };
}

/** The subagent peg row. */
export function pegsTip(helpers: Ranked[], total: number, per: number, days?: number): Tip {
  const kinds = ranked(helpers);
  const day = perDay(total, days);
  const rows: [string, string][] = kinds.slice(0, 5).map((h) => [h.label, fmt(h.value)]);
  rows.push(["Total", `${fmt(total)} subagents`]);
  if (day) rows.push(["Per worked day", day]);
  const lead = kinds[0];
  return {
    title: "Subagents",
    sub: `1 peg = ${fmt(per)} delegations`,
    rows,
    bar: lead ? { value: lead.value, max: Math.max(total, lead.value), label: `${lead.label}, your favourite helper` } : undefined,
    note: day ? `You delegated ${day} times a day.` : `You handed work to ${plural(total, "helper")} in this range.`,
    accent: "var(--violet)",
  };
}

/** The warehouse door: everything that came through it. */
export function doorTip(all: number, shownTools: number, top: Ranked | undefined, days?: number): Tip {
  const day = perDay(all, days);
  const rows: [string, string][] = [["Tool calls", fmt(all)], ["Top tools shown", fmt(shownTools)]];
  if (day) rows.push(["Per worked day", `${day} calls`]);
  if (top) rows.push(["Most used", `${shortTool(top.label)} (${pctOf(top.value, all)})`]);
  return {
    title: "The warehouse door",
    sub: "every tool call came in through here",
    rows,
    note: day ? `Roughly ${day} deliveries every worked day.` : `${fmt(all)} deliveries in this range.`,
    accent: "var(--slate)",
  };
}

