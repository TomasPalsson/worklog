import { BillingDetails } from "./BillingDetails";
import { MirresStatusIcon } from "./MirresStatusIcon";
import {
  customersFrom,
  customerStatusSplit,
  dateRangeLabel,
  goalGap,
  hrs,
  ledgerOrder,
  ledgerSegments,
  ledgerSummary,
  stripCustomer,
  totals,
} from "@/lib/mirresOverview";
import { STATUS_META, statusMeta } from "@/lib/mirresStatus";
import type { CustomerGroup, ProjectRow } from "@/lib/mirresOverview";
import type { MirresDay } from "@/lib/tempo_line_contract";

const GOAL = 70;

const nameOf = (p: ProjectRow) => stripCustomer(p.project, p.customer) ?? p.account_key;

function ProjectLine({ p }: { p: ProjectRow }) {
  const meta = statusMeta(p);
  return (
    <li className="mirres-proj">
      <MirresStatusIcon kind={meta.kind} />
      <span className="mirres-proj-name">{nameOf(p)}</span>
      {!p.warning && <span className="mirres-proj-type">{p.project_type ?? meta.label}</span>}
      <span className="mirres-proj-hours">{hrs(p.seconds)}</span>
      <span className="mirres-sr">{meta.label}</span>
    </li>
  );
}

function CustomerRow({ c }: { c: CustomerGroup }) {
  return (
    <li>
      <details className="mirres-ledger-row">
        <summary>
          <span className="mirres-ledger-name">{c.name}</span>
          <span className="mirres-ledger-hours">{hrs(c.seconds)}</span>
          <span className="mirres-ledger-chips">
            {customerStatusSplit(c).map((s) => (
              <span key={s.kind} className="mirres-chip" data-tone={STATUS_META[s.kind].tone}>
                <MirresStatusIcon kind={s.kind} size={12} />
                {hrs(s.seconds)} {STATUS_META[s.kind].label}
              </span>
            ))}
          </span>
        </summary>
        <ul className="mirres-proj-list">
          {c.projects.map((p) => (
            <ProjectLine key={p.account_key} p={p} />
          ))}
        </ul>
        {c.projects.map((p) => (
          <div key={p.account_key} className="mirres-card-person">
            <span className="mirres-card-person-name">{nameOf(p)}</span>
            <BillingDetails details={p.details} className="mirres-details" />
          </div>
        ))}
      </details>
    </li>
  );
}

/** The headline figure, the per-customer hours bar and the expandable customer list. */
export function MirresLedger({ days }: { days: MirresDay[] }) {
  const t = totals(days);
  const customers = customersFrom(days);
  const segments = ledgerSegments(customers);
  const pct = t.billablePercent;
  const gap = goalGap(t.billedSeconds, t.seconds, GOAL);
  return (
    <section className="reg-section mirres-ledger" aria-label="Billable ledger">
      <div className="mirres-figure-block">
        <span className="mirres-big" data-tone={pct !== null && pct >= GOAL ? "sage" : "fg"}>
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
        <div className="mirres-bar" role="img" aria-label={ledgerSummary(pct, t.seconds, segments, GOAL)}>
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
      <ul className="mirres-ledger-list">
        {ledgerOrder(customers).map((c) => (
          <CustomerRow key={c.name} c={c} />
        ))}
      </ul>
    </section>
  );
}
