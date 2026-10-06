import { BillingDetails } from "./BillingDetails";
import { MirresStatusIcon } from "./MirresStatusIcon";
import { customersFrom, stripCustomer } from "@/lib/mirresOverview";
import { statusMeta } from "@/lib/mirresStatus";
import type { CustomerGroup, ProjectRow } from "@/lib/mirresOverview";
import type { MirresDay } from "@/lib/tempo_line_contract";

const hrs = (seconds: number) => `${(seconds / 3600).toFixed(1)} h`;
const nameOf = (p: ProjectRow) => stripCustomer(p.project, p.customer) ?? p.account_key;

function ProjectLine({ p }: { p: ProjectRow }) {
  const meta = statusMeta(p);
  return (
    <li className="mirres-proj">
      <MirresStatusIcon kind={meta.kind} />
      <span className="mirres-proj-name">{nameOf(p)}</span>
      <span className="mirres-proj-type">{p.project_type ?? meta.label}</span>
      <span className="mirres-proj-hours">{hrs(p.seconds)}</span>
      <span className="mirres-sr">{meta.label}</span>
    </li>
  );
}

function CustomerCard({ c }: { c: CustomerGroup }) {
  const total = c.seconds || 1;
  return (
    <li className="mirres-card">
      <div className="mirres-card-head">
        <h3>{c.name}</h3>
        <span className="mirres-proj-hours">{hrs(c.seconds)}</span>
      </div>
      <div className="mirres-bar mirres-bar-mini" aria-hidden="true">
        {c.projects.map((p) => (
          <span
            key={p.account_key}
            className="mirres-seg"
            data-kind={statusMeta(p).kind}
            style={{ flexGrow: p.seconds / total }}
          />
        ))}
      </div>
      <ul className="mirres-proj-list">
        {c.projects.map((p) => (
          <ProjectLine key={p.account_key} p={p} />
        ))}
      </ul>
      <details className="mirres-card-more">
        <summary>People &amp; contract</summary>
        {c.projects.map((p) => (
          <div key={p.account_key} className="mirres-card-person">
            <span className="mirres-card-person-name">{nameOf(p)}</span>
            <BillingDetails details={p.details} className="mirres-details" />
            <span className="mirres-fix-meta">{p.tickets.join(", ")}</span>
          </div>
        ))}
      </details>
    </li>
  );
}

export function MirresCustomers({ days }: { days: MirresDay[] }) {
  return (
    <section className="reg-section" aria-labelledby="mirres-cust-h">
      <h2 id="mirres-cust-h">Customers</h2>
      <ul className="mirres-card-grid">
        {customersFrom(days).map((c) => (
          <CustomerCard key={c.name} c={c} />
        ))}
      </ul>
    </section>
  );
}
