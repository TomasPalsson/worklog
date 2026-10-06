import { MirresStatusIcon } from "./MirresStatusIcon";
import { customersFrom, dateRangeLabel, goalGap, hrs, ledgerOrder, ledgerSegments, totals } from "@/lib/mirresOverview";
import { STATUS_META, statusMeta } from "@/lib/mirresStatus";
import type { MirresDay } from "@/lib/tempo_line_contract";

const GOAL = 70;

/** The headline figure, the per-customer hours bar and its text legend. */
export function MirresLedger({ days }: { days: MirresDay[] }) {
  const t = totals(days);
  const customers = customersFrom(days);
  const order = ledgerOrder(customers).map((c) => ({ c, meta: statusMeta(c.projects[0]) }));
  const segments = ledgerSegments(customers);
  const pct = t.billablePercent;
  const gap = goalGap(t.billedSeconds, t.seconds, GOAL);
  return (
    <section className="reg-section mirres-ledger" aria-label="Billable ledger">
      <div className="mirres-figure-block">
        <span className="mirres-big" data-tone={pct !== null && pct >= GOAL ? "sage" : "amber"}>
          {pct === null ? "—" : `${pct}%`}
        </span>
        <span className="mirres-big-label">billable</span>
        <span className="mirres-goal-line">
          goal {GOAL}% · {hrs(t.seconds)} logged · {dateRangeLabel(days.map((d) => d.day))}
        </span>
        <span className="mirres-goal-line">
          {gap > 0 ? `${hrs(gap)} short of ${GOAL}%` : "on goal"}
        </span>
      </div>
      <div className="mirres-bar-wrap">
        <div className="mirres-bar" aria-hidden="true">
          {segments.map((s) => (
            <span
              key={`${s.customer}-${s.kind}`}
              className="mirres-seg"
              data-kind={s.kind}
              style={{ flexGrow: s.seconds }}
              title={`${s.customer} · ${hrs(s.seconds)} · ${STATUS_META[s.kind].label}`}
            />
          ))}
        </div>
        <span className="mirres-goal-tick" style={{ left: `${GOAL}%` }} aria-hidden="true">
          <span>{GOAL}%</span>
        </span>
      </div>
      <ul className="mirres-legend">
        {order.map(({ c, meta }) => (
          <li key={c.name}>
            <MirresStatusIcon kind={meta.kind} />
            <span>{c.name}</span>
            <span className="mirres-legend-hours">{hrs(c.seconds)}</span>
            <span className="mirres-sr">{meta.label}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
