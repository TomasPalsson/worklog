import type { PromptStats, Ranked } from "@/lib/stats_contract";
import { antennaColor, LEAD, politeness, rudeness, type BubbleSpec, type Mood } from "./PoliteBot";
import type { Tip } from "./tip";

const nf = new Intl.NumberFormat("en-US");

const pct = (v: number, of: number) => (of > 0 ? (v > 0 && v / of < 0.005 ? "<1%" : `${Math.round((v / of) * 100)}%`) : "0%");
const plural = (n: number, w: string) => `${nf.format(n)} ${w}${n === 1 ? "" : "s"}`;
const perDay = (v: number, days?: number) => (days && days > 0 ? (v / days).toFixed(1) : null);

/** What each bubble counts (mirrors worklog-core stats_prompt.rs). */
const COUNTS: Record<string, string> = {
  please: "prompts with the word \"please\"",
  thanks: "thanks / thank you / thx / ty",
  sorry: "prompts with the word \"sorry\"",
  swears: "fuck / shit / damn / wtf / crap (and words starting with them)",
  interrupts: "\"[Request interrupted by user]\" turns, not counted as prompts",
  slash: "slash-command turns, not counted as prompts",
  questions: "prompts ending in ?",
};

/** Tip for one speech bubble; `all` is every bubble, so rank = position by count. */
export function bubbleTip(b: BubbleSpec, all: BubbleSpec[], p: PromptStats, days?: number): Tip {
  const rank = [...all].sort((x, y) => y.n - x.n).findIndex((x) => x.key === b.key) + 1;
  const outside = b.key === "interrupts" || b.key === "slash"; // not part of `count`
  const day = perDay(b.n, days);
  const rows: [string, string][] = [[outside ? "Turns" : "Prompts containing it", nf.format(b.n)]];
  rows.push(outside ? ["Per 100 prompts", p.count > 0 ? ((b.n / p.count) * 100).toFixed(1) : "0"] : ["Share of prompts", pct(b.n, p.count)]);
  if (day) rows.push(["Per worked day", day]);
  rows.push(["What counts", COUNTS[b.key] ?? b.key]);
  const every = b.n > 0 && p.count >= b.n ? Math.round(p.count / b.n) : 0;
  let note: string;
  if (b.key === "swears") note = p.swears > 0 && politeness(p) > 0 ? `${(politeness(p) / p.swears).toFixed(1)} kind words for every swear.` : `${nf.format(p.swears)} swears against ${nf.format(politeness(p))} kind words.`;
  else if (outside) note = `That's ${(b.n / Math.max(1, p.count)).toFixed(2)} per prompt.`;
  else note = every > 1 ? `About 1 prompt in ${nf.format(every)} has this.` : "Practically every prompt has this.";
  return {
    title: b.text,
    sub: rank === 1 ? `the biggest of ${plural(all.length, "bubble")}` : `#${rank} of ${plural(all.length, "bubble")}`,
    rows,
    bar: { value: b.n, max: Math.max(p.count, b.n), label: `vs ${nf.format(p.count)} prompts` },
    note,
    accent: b.color,
  };
}

/** Tip for the bot itself: mood and how it is worked out. */
export function botTip(p: PromptStats, mood: Mood): Tip {
  const pol = politeness(p);
  const rude = rudeness(p);
  const total = pol + rude;
  return {
    title: `Politeness bot: ${mood}`,
    sub: LEAD[mood],
    rows: [
      ["Kind words", `${nf.format(pol)} (please + thanks + sorry)`],
      ["Swears", nf.format(p.swears)],
      ["Interruptions", `${nf.format(p.interrupts)} (5 count as 1)`],
      ["Politeness score", String(pol)],
      ["Rudeness score", (Math.round(rude * 10) / 10).toString()],
      ["Kindness share", total > 0 ? pct(pol, total) : "no signal yet"],
    ],
    bar: { value: pol, max: total, label: "politeness / (politeness + rudeness)" },
    note: "85%+ delighted, 60%+ happy, 40%+ neutral, 20%+ wary, below that grumpy.",
    accent: antennaColor(mood),
  };
}

/** Tip for the tape measure: average vs longest prompt. */
export function tapeTip(p: PromptStats): Tip {
  const avg = Math.round(p.avg_chars);
  const times = avg > 0 ? Math.round(p.longest_chars / avg) : 0;
  return {
    title: "Prompt length",
    sub: `${nf.format(p.count)} prompts measured`,
    rows: [
      ["Average", `${nf.format(avg)} chars`],
      ["Longest", `${nf.format(p.longest_chars)} chars`],
      ["Average vs longest", pct(p.avg_chars, p.longest_chars)],
    ],
    bar: { value: avg, max: p.longest_chars, label: "average out of the longest" },
    note: times > 1 ? `Your longest prompt was ${nf.format(times)}× the average.` : "Your longest prompt is barely longer than the average.",
    accent: "var(--amber)",
  };
}

/** Tip for one opener word. `list` is p.top_openers. */
export function openerTip(o: Ranked, list: Ranked[], p: PromptStats, days?: number): Tip {
  const rank = list.findIndex((x) => x.label === o.label) + 1;
  const max = Math.max(0, ...list.map((x) => x.value));
  const covered = list.reduce((s, x) => s + x.value, 0);
  const day = perDay(o.value, days);
  const rows: [string, string][] = [
    ["Prompts starting with it", nf.format(o.value)],
    ["Share of prompts", pct(o.value, p.count)],
  ];
  if (day) rows.push(["Per worked day", day]);
  rows.push(["Top openers together", pct(covered, p.count)]);
  return {
    title: `"${o.label}"`,
    sub: rank === 1 ? `your go-to first word, #1 of ${nf.format(list.length)}` : `#${rank} of ${plural(list.length, "opener")}`,
    rows,
    bar: { value: o.value, max, label: rank === 1 ? "the top opener" : `vs "${list[0]?.label}"` },
    note: rank === 1 ? `${pct(o.value, p.count)} of your prompts start with "${o.label}".` : `"${list[0]?.label}" beats it by ${nf.format((list[0]?.value ?? 0) - o.value)} prompts.`,
    accent: "var(--sage)",
  };
}

