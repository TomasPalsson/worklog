import "@/app/stats/receipt.css";
import type { DailyStat, StatsReport } from "@/lib/stats_contract";
import { formatDuration, shortMonthDay } from "@/lib/format";
import { tipProps, type Tip } from "./tip";
import { rangeDays } from "./TicketMetro";
import { dayLabel } from "./time-utils";

export interface ReceiptLine {
  qty: string;
  item: string;
  amount: string;
  title: string;
  /** Raw total: a count, or seconds when kind is "seconds". */
  value: number;
  kind: "count" | "seconds";
  /** Matching per-day column in report.daily, when there is one. */
  metric?: keyof DailyStat;
  /** One plain sentence on what the line counts. */
  what: string;
}

export const fmtInt = (n: number) => Math.round(n).toLocaleString("en-US");
export const fmtHours = (seconds: number) => `${(seconds / 3600).toFixed(1)}h`;

export function receiptLines(r: StatsReport): ReceiptLine[] {
  const t = r.totals;
  const count = (item: string, n: number, what: string, metric?: keyof DailyStat): ReceiptLine => ({ qty: "1×", item, amount: fmtInt(n), title: `${item}: ${n.toLocaleString("en-US")}`, value: n, kind: "count", metric, what });
  const hours = (item: string, s: number, what: string, metric?: keyof DailyStat): ReceiptLine => ({ qty: "1×", item, amount: fmtHours(s), title: `${item}: ${(s / 3600).toFixed(2)} hours`, value: s, kind: "seconds", metric, what });
  const lines = [
    hours("Hours worked", t.work_seconds, "time inside work blocks, personal and ignored time excluded", "work_seconds"),
    count("Claude prompts", t.prompts, "messages you sent to Claude Code", "prompts"),
    count("Tool calls", t.tool_calls, "actions Claude took on your behalf (edits, reads, commands)", "tool_calls"),
    count("Subagents", t.helpers, "helper agents Claude spun up"),
    count("Shell commands", t.shell_commands, "commands you ran in a terminal", "shell"),
    count("Commits", t.commits, "git commits you made", "commits"),
    count("PRs", t.prs, "pull requests you opened or merged"),
    count("Slack messages", t.slack_messages, "messages you sent in Slack", "slack"),
    hours("Meetings", t.meeting_seconds, "calendar time spent in meetings", "meeting_seconds"),
    hours("Browser", t.browser_minutes * 60, "minutes the browser was active", "browser_minutes"),
  ];
  const top = r.tools[0];
  if (top) lines.push(count(top.label, top.value, "calls of your most-used Claude tool"));
  return lines;
}

const nf = new Intl.NumberFormat("en-US");
const pct = (a: number, b: number) => (b > 0 ? Math.round((a / b) * 100) : 0);

/** One reading of a line's value, in its own unit. */
const show = (l: Pick<ReceiptLine, "kind">, v: number) => (l.kind === "seconds" ? formatDuration(v) : nf.format(Math.round(v)));
const rate = (v: number) => (v >= 10 ? nf.format(Math.round(v)) : (Math.round(v * 10) / 10).toString());

/** A "fun" closing sentence from the line's own numbers; undefined when there is nothing honest to say. */
function lineNote(r: StatsReport, l: ReceiptLine): string | undefined {
  const t = r.totals;
  const workMin = t.work_seconds / 60;
  const every = (what: string) => (l.value > 0 && workMin > 0 ? `≈ one ${what} every ${workMin / l.value < 1 ? `${rate((workMin * 60) / l.value)} seconds` : `${rate(workMin / l.value)} minutes`} you were at work` : undefined);
  switch (l.item) {
    case "Hours worked":
      return t.work_seconds > 0 ? `≈ ${rate(t.work_seconds / 28800)} full 8-hour days` : undefined;
    case "Claude prompts":
      return every("prompt");
    case "Tool calls":
      return every("tool call");
    case "Shell commands":
      return every("command");
    case "Slack messages":
      return every("message");
    case "Subagents":
      return l.value > 0 ? `${rate(t.tool_calls / l.value)} tool calls per subagent` : undefined;
    case "Commits":
      return l.value > 0 && t.work_seconds > 0 ? `a commit every ${rate(t.work_seconds / 3600 / l.value)} hours of work` : undefined;
    case "PRs":
      return l.value > 0 ? `${rate(t.commits / l.value)} commits per PR` : undefined;
    case "Meetings":
      return t.work_seconds > 0 ? `${pct(l.value, t.work_seconds)}% of your work time was spent in meetings` : undefined;
    case "Browser":
      return t.work_seconds > 0 ? `${pct(l.value, t.work_seconds)}% as long as your work time` : undefined;
    default:
      return t.tool_calls > 0 ? `${pct(l.value, t.tool_calls)}% of all tool calls` : undefined;
  }
}

/** Hover card for a receipt line, with per-day figures from the report. */
export function receiptTip(r: StatsReport, l: ReceiptLine): Tip {
  const t = r.totals;
  const cal = Math.max(1, rangeDays(r.from, r.to));
  const rows: [string, string][] = [["Total", show(l, l.value)]];
  if (t.days_worked > 0) rows.push(["Per worked day", show(l, l.value / t.days_worked)]);
  rows.push(["Per calendar day", show(l, l.value / cal)]);
  const scale = l.metric === "browser_minutes" ? 60 : 1;
  const best = l.metric
    ? (r.daily ?? []).reduce<{ day: string; v: number } | null>((b, d) => {
        const v = (d[l.metric!] as number) * scale;
        return v > (b?.v ?? 0) ? { day: d.day, v } : b;
      }, null)
    : null;
  if (best) rows.push(["Best day", `${dayLabel(best.day)}: ${show(l, best.v)}`]);
  rows.push(["Counts", l.what]);
  return { title: l.item, sub: `${shortMonthDay(r.from)} → ${shortMonthDay(r.to)}`, rows, note: lineNote(r, l), accent: "var(--terracotta)" };
}

const hrs = (s: number) => `${(s / 3600).toFixed(2)} hours`;

/** Hover card for the totals block: SUBTOTAL, TAX, DISCOUNT and TOTAL. */
export function totalsTip(r: StatsReport, kind: "total" | "subtotal" | "tax" | "discount"): Tip {
  const t = r.totals;
  const all = t.work_seconds + t.personal_seconds + t.ignored_seconds;
  const [title, secs, what] = {
    total: ["TOTAL", t.work_seconds, "work hours, the same as the subtotal"],
    subtotal: ["SUBTOTAL work hours", t.work_seconds, "time inside work blocks"],
    tax: ["TAX (ignored time)", t.ignored_seconds, "time you told worklog to ignore, never billed"],
    discount: ["DISCOUNT (personal)", t.personal_seconds, "time in personal blocks, kept off the work total"],
  }[kind] as [string, number, string];
  const rows: [string, string][] = [["Exact", hrs(secs)], ["Readable", formatDuration(secs)], ["Counts", what]];
  if (t.days_worked > 0) rows.splice(2, 0, ["Per worked day", formatDuration(secs / t.days_worked)]);
  return {
    title,
    sub: `${shortMonthDay(r.from)} → ${shortMonthDay(r.to)}`,
    rows,
    bar: all > 0 ? { value: secs, max: all, label: `${pct(secs, all)}% of all tracked time` } : undefined,
    note: kind === "total" || kind === "subtotal" ? (t.work_seconds > 0 ? `${pct(t.work_seconds, all)}% of the time worklog saw was work` : undefined) : secs === 0 ? "None at all this period." : t.work_seconds > 0 ? `${pct(secs, t.work_seconds)}% on top of ${formatDuration(t.work_seconds)} of work` : `${formatDuration(secs)} with no work logged`,
    accent: "var(--terracotta)",
  };
}

export function stampTip(r: StatsReport): Tip {
  const { synced_seconds: s, unsynced_seconds: u } = r.sync;
  const all = s + u;
  return {
    title: u === 0 ? "PAID: everything is synced" : "UNPAID: hours waiting for Tempo",
    sub: `${shortMonthDay(r.from)} → ${shortMonthDay(r.to)}`,
    rows: [["Synced to Tempo", formatDuration(s)], ["Not synced", formatDuration(u)], ["Exported blocks", nf.format(r.sync.exported_blocks)]],
    bar: all > 0 ? { value: s, max: all, label: `${pct(s, all)}% synced` } : undefined,
    note: u === 0 ? "Nothing left to push: the ledger is clean." : `${formatDuration(u)} still to sync before this is fully paid`,
    accent: u === 0 ? "var(--sage)" : "var(--terracotta)",
  };
}

export const barcodeTip = (r: StatsReport): Tip => ({
  title: "A barcode of your period",
  sub: `${r.from} → ${r.to}`,
  note: "Drawn from the dates alone, so the same period always prints the same bars. Not scannable :)",
  accent: "var(--fg)",
});

/** Deterministic bar widths (1..3 units) from a string, FNV-1a + xorshift. */
export function barcode(seed: string, n = 40): number[] {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619) >>> 0;
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    h ^= h << 13;
    h >>>= 0;
    h ^= h >>> 17;
    h ^= h << 5;
    h >>>= 0;
    out.push((h % 3) + 1);
  }
  return out;
}

export function isPaid(r: StatsReport): boolean {
  return r.sync.unsynced_seconds === 0;
}

export function insight(r: StatsReport): string {
  const t = r.totals;
  const perDay = t.days_worked > 0 ? t.work_seconds / t.days_worked : 0;
  return `${fmtHours(t.work_seconds)} rung up over ${t.days_worked} ${t.days_worked === 1 ? "day" : "days"}, about ${fmtHours(perDay)} a day.`;
}

export function ariaLabel(r: StatsReport): string {
  const t = r.totals;
  return `Receipt for ${r.from} to ${r.to}: total ${fmtHours(t.work_seconds)} work over ${t.days_worked} days, ${fmtInt(t.prompts)} prompts, ${fmtInt(t.tool_calls)} tool calls. ${isPaid(r) ? "Paid" : `Unpaid, ${fmtHours(r.sync.unsynced_seconds)} unsynced to Tempo`}.`;
}

export function isEmpty(r: StatsReport): boolean {
  return r.totals.work_seconds === 0 && r.totals.days_worked === 0;
}

function Row({ qty, item, amount, big, tip }: Partial<ReceiptLine> & { big?: boolean; tip: Tip }) {
  return (
    <div className={`sx-receipt-line${big ? " sx-receipt-total" : ""}`} role="group" tabIndex={0} aria-label={`${item}: ${amount}`} {...tipProps(tip)}>
      <span>
        {qty ? `${qty} ` : ""}
        {item}
      </span>
      <span className="sx-receipt-dots" aria-hidden="true" />
      <span className="sx-receipt-amt">{amount}</span>
    </div>
  );
}

function Awning() {
  return (
    <svg className="sx-receipt-awning" viewBox="0 0 44 24" aria-hidden="true">
      <path d="M4 2h36l3 10H1z" fill="var(--terracotta)" />
      <path d="M12 2l-2 10M20 2l-1 10M28 2l1 10M36 2l2 10" stroke="var(--bg-raised)" strokeWidth="2" />
      <path d="M1 12q3.5 4 7 0q3.5 4 7 0q3.5 4 7 0q3.5 4 7 0q3.5 4 7 0q3.5 4 7 0" fill="var(--terracotta)" />
      <path d="M6 16v7h32v-7" fill="none" stroke="var(--border-strong)" />
    </svg>
  );
}

function Barcode({ seed, tip }: { seed: string; tip: Tip }) {
  let x = 0;
  const rects = barcode(seed).map((w, i) => {
    const r = i % 2 === 0 ? <rect key={i} x={x} y={0} width={w} height={30} fill="currentColor" /> : null;
    x += w;
    return r;
  });
  return (
    <svg className="sx-receipt-bars" viewBox={`0 0 ${x} 30`} preserveAspectRatio="none" role="img" tabIndex={0} aria-label={tip.title} {...tipProps(tip)}>
      {rects}
    </svg>
  );
}

function Stamp({ unsynced, tip }: { unsynced: number; tip: Tip }) {
  const paid = unsynced === 0;
  return (
    <div
      className={`sx-receipt-stamp ${paid ? "sx-receipt-paid" : "sx-receipt-unpaid"}`}
      tabIndex={0}
      aria-label={tip.title}
      {...tipProps(tip)}
    >
      {paid ? "PAID" : "UNPAID"}
      {!paid && <small>{fmtHours(unsynced)} unsynced to Tempo</small>}
    </div>
  );
}

export function PeriodReceipt({ report }: { report: StatsReport }) {
  if (isEmpty(report)) {
    return (
      <div className="sx-receipt-wrap">
        <div className="sx-receipt-plate" role="img" aria-label="Receipt: nothing built yet">
          nothing built yet
        </div>
      </div>
    );
  }
  const t = report.totals;
  return (
    <div className="sx-receipt-wrap">
      <p className="sx-receipt-insight">{insight(report)}</p>
      <div className="sx-receipt-stage">
        <div className="sx-receipt-slot" aria-hidden="true" />
        <div className="sx-receipt-feed">
          <div className="sx-receipt" role="img" aria-label={ariaLabel(report)}>
            <div className="sx-receipt-head">
              <div className="sx-receipt-shop">WORKLOG MARKET</div>
              <Awning />
              <div>
                {shortMonthDay(report.from)} → {shortMonthDay(report.to)}
              </div>
              <div>till #{t.days_worked}</div>
            </div>
            <hr className="sx-receipt-rule" />
            {receiptLines(report).map((l) => (
              <Row key={l.item} {...l} tip={receiptTip(report, l)} />
            ))}
            <hr className="sx-receipt-rule" />
            <Row item="SUBTOTAL work hours" amount={fmtHours(t.work_seconds)} tip={totalsTip(report, "subtotal")} />
            <Row item="TAX (ignored time)" amount={fmtHours(t.ignored_seconds)} tip={totalsTip(report, "tax")} />
            <Row item="DISCOUNT (personal)" amount={fmtHours(t.personal_seconds)} tip={totalsTip(report, "discount")} />
            <hr className="sx-receipt-rule" />
            <Row big item="TOTAL" amount={fmtHours(t.work_seconds)} tip={totalsTip(report, "total")} />
            <div className="sx-receipt-thanks">Thank you for working!</div>
            <Barcode seed={`${report.from}→${report.to}`} tip={barcodeTip(report)} />
            <div className="sx-receipt-small">please come again</div>
            <Stamp unsynced={report.sync.unsynced_seconds} tip={stampTip(report)} />
          </div>
        </div>
      </div>
      <p className="sx-receipt-legend">Each line is a count from your report; TOTAL is work hours.</p>
    </div>
  );
}
