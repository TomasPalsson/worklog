"use client";

import { useMemo, useState, type CSSProperties } from "react";
import "@/app/stats/tickets.css";
import type { TicketStat } from "@/lib/stats_contract";
import { formatDuration } from "@/lib/format";
import { tipProps } from "./tip";
import {
  buildCols,
  cellTip,
  dayTotals,
  fmtHours,
  headline,
  maxTicketDay,
  rowTip,
  shadeBucket,
  shadeRanges,
  sortTickets,
  ticketDays,
  ticketStatsLine,
  type Col,
  type SortMode,
} from "./timelineLib";

const SHOW = 12;

interface Props {
  tickets: TicketStat[];
  from: string;
  to: string;
  today: string;
  totalWork: number;
}

const catClass = (c: string | null) => (c === "new" || c === "indeterminate" || c === "done" ? c : "none");
const colCls = (c: Col) => `${c.weekend ? " tt-we" : ""}${c.monday ? " tt-mon" : ""}${c.today ? " tt-today" : ""}`;

function Header({ cols }: { cols: Col[] }) {
  return (
    <div className="tt-row tt-hdr" role="row">
      <div className="tt-info tt-info-hdr" role="columnheader">Ticket</div>
      {cols.map((c) => (
        <div key={c.day} className={`tt-col${colCls(c)}`} role="columnheader" aria-label={c.day}>
          <span className="tt-mo">{c.month ?? ""}</span>
          <span className="tt-dom">{c.dom}</span>
          <span className="tt-dow">{c.initial}</span>
        </div>
      ))}
    </div>
  );
}

function Cell({ t, c, s, max, edge }: { t: TicketStat; c: Col; s: number | undefined; max: number; edge: string }) {
  const cls = `tt-cell${colCls(c)}${edge}`;
  if (s === undefined) {
    return (
      <div className={cls} role="cell">
        <i className="tt-dot" />
      </div>
    );
  }
  return (
    <div className={cls} role="cell">
      <span
        className={`tt-fill tt-s${shadeBucket(s, max)}`}
        tabIndex={0}
        aria-label={`${t.key}, ${c.day}: ${formatDuration(s)}`}
        {...tipProps(cellTip(t, c.day, s))}
      >
        <span className="tt-num">{fmtHours(s)}</span>
      </span>
    </div>
  );
}

function TicketRow({ t, cols, max, totalWork }: { t: TicketStat; cols: Col[]; max: number; totalWork: number }) {
  const days = ticketDays(t);
  const by = new Map(days.map((d) => [d.day, d.seconds]));
  const first = days.reduce((a, d) => (d.day < a ? d.day : a), days[0].day);
  const last = days.reduce((a, d) => (d.day > a ? d.day : a), days[0].day);
  const line = ticketStatsLine(t, totalWork);
  const cat = catClass(t.status_category);
  return (
    <div className="tt-row" role="row" data-ticket={t.key}>
      <div
        className="tt-info"
        role="rowheader"
        tabIndex={0}
        aria-label={`${t.key}${t.summary ? `, ${t.summary}` : ""}: ${line}`}
        {...tipProps(rowTip(t, totalWork))}
      >
        <div className="tt-top">
          <span className={`tt-key tt-c-${cat}`}>{t.key}</span>
          {t.status && <span className={`tt-chip tt-c-${cat}`}>{t.status}</span>}
        </div>
        {t.summary && <div className="tt-sum">{t.summary}</div>}
        <div className="tt-stats">{line}</div>
      </div>
      {cols.map((c) => {
        const edge = c.day < first || c.day > last ? "" : ` tt-in${c.day === first ? " tt-first" : ""}${c.day === last ? " tt-last" : ""}`;
        return <Cell key={c.day} t={t} c={c} s={by.get(c.day)} max={max} edge={edge} />;
      })}
    </div>
  );
}

function TotalRow({ cols, totals, count }: { cols: Col[]; totals: number[]; count: number }) {
  const peak = Math.max(...totals, 1);
  return (
    <div className="tt-row tt-total" role="row">
      <div className="tt-info" role="rowheader">
        <span className="tt-key tt-c-none">All tickets</span>
        <span className="tt-stats">
          {formatDuration(totals.reduce((a, b) => a + b, 0))} on the {count} shown
        </span>
      </div>
      {cols.map((c, i) => (
        <div key={c.day} className={`tt-cell${colCls(c)}`} role="cell">
          {totals[i] > 0 && (
            <>
              <i className="tt-tbar" style={{ height: `${Math.max(8, (totals[i] / peak) * 100)}%` }} />
              <span className="tt-tnum">{fmtHours(totals[i])}</span>
            </>
          )}
        </div>
      ))}
    </div>
  );
}

function Legend({ max, mode, setMode }: { max: number; mode: SortMode; setMode: (m: SortMode) => void }) {
  return (
    <div className="tt-bar">
      <div className="tt-legend" aria-hidden="true">
        {shadeRanges(max).map((r, i) => (
          <span key={i} className="tt-lg">
            <i className={`tt-sw tt-s${i + 1}`} />
            {r}
          </span>
        ))}
        <span className="tt-lg"><i className="tt-sw tt-sw-we" />weekend</span>
        <span className="tt-lg"><i className="tt-sw tt-sw-today" />today</span>
      </div>
      <div className="tt-seg" role="group" aria-label="Sort tickets">
        <button type="button" aria-pressed={mode === "hours"} onClick={() => setMode("hours")}>by hours</button>
        <button type="button" aria-pressed={mode === "start"} onClick={() => setMode("start")}>by start date</button>
      </div>
    </div>
  );
}

export function TicketTimeline({ tickets, from, to, today, totalWork }: Props) {
  const [mode, setMode] = useState<SortMode>("hours");
  const [all, setAll] = useState(false);
  const cols = useMemo(() => buildCols(from, to, today), [from, to, today]);
  const sorted = useMemo(() => sortTickets(tickets, mode), [tickets, mode]);

  if (tickets.length === 0 || cols.length === 0) {
    return (
      <div className="tt-card">
        <p className="tt-empty">No ticket work in this range yet.</p>
      </div>
    );
  }

  const shown = all ? sorted : sorted.slice(0, SHOW);
  const max = maxTicketDay(tickets);
  const cw = cols.length <= 45 ? 28 : cols.length <= 90 ? 22 : 18;
  const style = { "--tt-cols": cols.length, "--tt-w": `${cw}px` } as CSSProperties;

  return (
    <div className="tt-card">
      <h3 className="tt-head">{headline(tickets)}</h3>
      <p className="tt-sub">Each row is a ticket, each square a day. Darker = more hours that day; the number is hours.</p>
      <Legend max={max} mode={mode} setMode={setMode} />
      <div className="tt-scroll">
        <div
          className={`tt-grid${cw < 22 ? " tt-compact" : ""}`}
          style={style}
          role="table"
          aria-label={`Ticket timeline: ${headline(tickets)}`}
        >
          <Header cols={cols} />
          {shown.map((t) => (
            <TicketRow key={t.key} t={t} cols={cols} max={max} totalWork={totalWork} />
          ))}
          <TotalRow cols={cols} totals={dayTotals(shown, cols)} count={shown.length} />
        </div>
      </div>
      {sorted.length > SHOW && (
        <button type="button" className="tt-more" onClick={() => setAll((v) => !v)}>
          {all ? `Show top ${SHOW}` : `Show all ${sorted.length} tickets`}
        </button>
      )}
    </div>
  );
}
