import { ChevronRight } from "lucide-react";
import { BillingDetails } from "./BillingDetails";
import { MirresHover } from "./MirresHover";
import { MirresStatusIcon } from "./MirresStatusIcon";
import {
  barPercent,
  customersFrom,
  customerStatusSplit,
  dateRangeLabel,
  goalGap,
  hrs,
  keyKinds,
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
const KEY_LABEL = {
  billable: "billable",
  included: "covered by contract",
  other: "other (fixed price / internal)",
  missing: "contract missing",
} as const;

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

function CustomerRow({ c, max }: { c: CustomerGroup; max: number }) {
  const n = c.projects.length;
  return (
    <li data-customer={c.name}>
      <details className="mirres-ledger-row">
        <summary>
          <ChevronRight size={14} className="mirres-chev" aria-hidden="true" />
          <span className="mirres-ledger-name">{c.name}</span>
          <span className="mirres-ledger-hours">
            {hrs(c.seconds)}
            <span className="mirres-ledger-count">{n} {n === 1 ? "project" : "projects"}</span>
          </span>
          <span className="mirres-mini" aria-hidden="true">
            <span className="mirres-mini-fill" style={{ width: `${barPercent(c.seconds, max)}%` }}>
              {customerStatusSplit(c).map((s) => (
                <span key={s.kind} className="mirres-seg" data-kind={s.kind} style={{ flexGrow: s.seconds }} />
              ))}
            </span>
          </span>
          <span className="mirres-ledger-chips">
            {customerStatusSplit(c).map((s) => (
              <span key={s.kind} className="mirres-chip" data-tone={STATUS_META[s.kind].tone}>
                <span className="mirres-swatch" data-kind={s.kind} aria-hidden="true" />
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
  const max = Math.max(0, ...customers.map((c) => c.seconds));
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
      <MirresHover>
      <div className="mirres-bar-wrap">
        <div className="mirres-bar" role="img" aria-label={ledgerSummary(pct, t.seconds, segments, GOAL)}>
          {segments.map((s) => (
            <span
              key={`${s.customer}-${s.kind}`}
              className="mirres-seg"
              data-kind={s.kind}
              data-customer={s.customer}
              style={{ flexGrow: s.seconds }}
              title={`${s.customer} · ${hrs(s.seconds)} · ${STATUS_META[s.kind].label}`}
            />
          ))}
        </div>
        <span className="mirres-goal-tick" style={{ left: `${GOAL}%` }} aria-hidden="true">
          <span>{GOAL}%</span>
        </span>
      </div>
      <ul className="mirres-key" aria-hidden="true">
        {keyKinds(segments).map((k) => (
          <li key={k}>
            <span className="mirres-swatch" data-kind={k === "other" ? "fixed" : k} />
            {KEY_LABEL[k]}
          </li>
        ))}
      </ul>
      <ul className="mirres-ledger-list">
        {ledgerOrder(customers).map((c) => (
          <CustomerRow key={c.name} c={c} max={max} />
        ))}
      </ul>
      </MirresHover>
    </section>
  );
}
