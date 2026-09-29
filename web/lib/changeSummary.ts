// Turns the raw change log into what the Owner actually wants to read:
// one entry per block with its *net* change per field. A block rebuild
// clears a description and Claude rewrites it a moment later — two raw
// rows ("X → —", "— → Y") that are really one edit ("X → Y"), and a
// rewrite back to the same text is no change at all.

import type { BlockChange, ChangeSource } from "@/lib/deildir";

/** Customer, deild and split all carry the same "who is billed" string. */
export type ChangeKind = "description" | "billing";

export interface NetChange {
  kind: ChangeKind;
  old: string | null;
  new: string | null;
  sources: ChangeSource[];
}

export interface BlockChangeGroup {
  day: string;
  started_at: string;
  changes: NetChange[];
}

export interface DayChangeGroup {
  day: string;
  blocks: BlockChangeGroup[];
}

const kindOf = (c: BlockChange): ChangeKind => (c.field === "description" ? "description" : "billing");
/** A blanked value is stored as "" by some writers and NULL by others. */
const blank = (v: string | null) => (v === null || v.trim() === "" ? null : v);

/** Newest day first; blocks within a day in clock order. */
export function summariseChanges(changes: BlockChange[]): DayChangeGroup[] {
  const blocks = new Map<string, BlockChangeGroup>();
  for (const c of [...changes].sort((a, b) => a.id - b.id)) {
    const key = `${c.day}|${c.started_at}`;
    let block = blocks.get(key);
    if (!block) {
      block = { day: c.day, started_at: c.started_at, changes: [] };
      blocks.set(key, block);
    }
    const kind = kindOf(c);
    const net = block.changes.find((n) => n.kind === kind);
    if (!net) {
      block.changes.push({ kind, old: blank(c.old), new: blank(c.new), sources: [c.source] });
    } else {
      net.new = blank(c.new);
      // Ordered by last touch, so the final entry is who acted last.
      net.sources = [...net.sources.filter((s) => s !== c.source), c.source];
    }
  }

  const days = new Map<string, BlockChangeGroup[]>();
  for (const block of blocks.values()) {
    block.changes = block.changes
      .filter((n) => n.old !== n.new)
      .sort((a, b) => (a.kind === b.kind ? 0 : a.kind === "description" ? -1 : 1));
    if (block.changes.length === 0) continue;
    days.set(block.day, [...(days.get(block.day) ?? []), block]);
  }
  return [...days.entries()]
    .sort(([a], [b]) => b.localeCompare(a))
    .map(([day, bs]) => ({
      day,
      blocks: bs.sort((a, b) => Date.parse(a.started_at) - Date.parse(b.started_at)),
    }));
}

export function countBlocks(days: DayChangeGroup[]): number {
  return days.reduce((n, d) => n + d.blocks.length, 0);
}

export interface DiffPart {
  text: string;
  kind: "same" | "add" | "del";
}

// ponytail: O(n·m) LCS table; descriptions are a sentence or two. Past
// this many cells it falls back to a whole-text swap.
const MAX_DIFF_CELLS = 40_000;

/** Word-level diff; whitespace is kept as its own tokens so joining the
 * parts back reproduces both inputs exactly. */
export function wordDiff(a: string, b: string): DiffPart[] {
  const x = a.split(/(\s+)/).filter(Boolean);
  const y = b.split(/(\s+)/).filter(Boolean);
  if (x.length * y.length > MAX_DIFF_CELLS) {
    return [
      { text: a, kind: "del" },
      { text: b, kind: "add" },
    ];
  }
  // lcs[i][j] = LCS length of x[i..] and y[j..]
  const lcs = Array.from({ length: x.length + 1 }, () => new Array<number>(y.length + 1).fill(0));
  for (let i = x.length - 1; i >= 0; i--) {
    for (let j = y.length - 1; j >= 0; j--) {
      lcs[i][j] = x[i] === y[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const out: DiffPart[] = [];
  const push = (text: string, kind: DiffPart["kind"]) => {
    const last = out[out.length - 1];
    if (last && last.kind === kind) last.text += text;
    else out.push({ text, kind });
  };
  let i = 0;
  let j = 0;
  while (i < x.length || j < y.length) {
    if (i < x.length && j < y.length && x[i] === y[j]) {
      push(x[i++], "same");
      j++;
    } else if (i < x.length && (j === y.length || lcs[i + 1][j] >= lcs[i][j + 1])) {
      // Deletions first so a swap reads "old → new".
      push(x[i++], "del");
    } else {
      push(y[j++], "add");
    }
  }
  return out;
}

/** Share of the new text's non-space characters that survived the edit.
 * Below ~0.4 an inline diff reads as confetti — show before/after instead. */
export function overlap(parts: DiffPart[]): number {
  const len = (k: DiffPart["kind"]) =>
    parts.filter((p) => p.kind === k).reduce((n, p) => n + p.text.replace(/\s/g, "").length, 0);
  const same = len("same");
  const total = same + Math.max(len("add"), len("del"));
  return total === 0 ? 1 : same / total;
}
