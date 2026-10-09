import "@/app/stats/receipt.css";
import type { StatsReport } from "@/lib/stats_contract";
import { shortMonthDay } from "@/lib/format";

export interface ReceiptLine {
  qty: string;
  item: string;
  amount: string;
  title: string;
}

export const fmtInt = (n: number) => Math.round(n).toLocaleString("en-US");
export const fmtHours = (seconds: number) => `${(seconds / 3600).toFixed(1)}h`;

export function receiptLines(r: StatsReport): ReceiptLine[] {
  const t = r.totals;
  const count = (item: string, n: number): ReceiptLine => ({ qty: "1×", item, amount: fmtInt(n), title: `${item}: ${n.toLocaleString("en-US")}` });
  const hours = (item: string, s: number): ReceiptLine => ({ qty: "1×", item, amount: fmtHours(s), title: `${item}: ${(s / 3600).toFixed(2)} hours` });
  const lines = [
    hours("Hours worked", t.work_seconds),
    count("Claude prompts", t.prompts),
    count("Tool calls", t.tool_calls),
    count("Subagents", t.helpers),
    count("Shell commands", t.shell_commands),
    count("Commits", t.commits),
    count("PRs", t.prs),
    count("Slack messages", t.slack_messages),
    hours("Meetings", t.meeting_seconds),
    hours("Browser", t.browser_minutes * 60),
  ];
  const top = r.tools[0];
  if (top) lines.push(count(top.label, top.value));
  return lines;
}

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

function Row({ qty, item, amount, title, big }: Partial<ReceiptLine> & { big?: boolean }) {
  return (
    <div className={`sx-receipt-line${big ? " sx-receipt-total" : ""}`} title={title}>
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

function Barcode({ seed }: { seed: string }) {
  let x = 0;
  const rects = barcode(seed).map((w, i) => {
    const r = i % 2 === 0 ? <rect key={i} x={x} y={0} width={w} height={30} fill="currentColor" /> : null;
    x += w;
    return r;
  });
  return (
    <svg className="sx-receipt-bars" viewBox={`0 0 ${x} 30`} preserveAspectRatio="none" aria-hidden="true">
      {rects}
    </svg>
  );
}

function Stamp({ unsynced }: { unsynced: number }) {
  const paid = unsynced === 0;
  return (
    <div
      className={`sx-receipt-stamp ${paid ? "sx-receipt-paid" : "sx-receipt-unpaid"}`}
      title={paid ? "Everything is synced to Tempo" : `${(unsynced / 3600).toFixed(2)} hours not synced to Tempo`}
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
              <Row key={l.item} {...l} />
            ))}
            <hr className="sx-receipt-rule" />
            <Row item="SUBTOTAL work hours" amount={fmtHours(t.work_seconds)} title={`${(t.work_seconds / 3600).toFixed(2)} hours of work`} />
            <Row item="TAX (ignored time)" amount={fmtHours(t.ignored_seconds)} title={`${(t.ignored_seconds / 3600).toFixed(2)} hours ignored`} />
            <Row item="DISCOUNT (personal)" amount={fmtHours(t.personal_seconds)} title={`${(t.personal_seconds / 3600).toFixed(2)} hours personal`} />
            <hr className="sx-receipt-rule" />
            <Row big item="TOTAL" amount={fmtHours(t.work_seconds)} />
            <div className="sx-receipt-thanks">Thank you for working!</div>
            <Barcode seed={`${report.from}→${report.to}`} />
            <div className="sx-receipt-small">please come again</div>
            <Stamp unsynced={report.sync.unsynced_seconds} />
          </div>
        </div>
      </div>
      <p className="sx-receipt-legend">Each line is a count from your report; TOTAL is work hours.</p>
    </div>
  );
}
