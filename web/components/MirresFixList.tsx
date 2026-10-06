import { ContractMissingIcon } from "./mirres-art";
import { AllClearSpot } from "./mirres-spots";
import { projectsFrom, stripCustomer, warningHelp } from "@/lib/mirresOverview";
import { statusKind } from "@/lib/mirresStatus";
import type { ProjectRow } from "@/lib/mirresOverview";
import type { MirresDay } from "@/lib/tempo_line_contract";

const hrs = (seconds: number) => `${(seconds / 3600).toFixed(1)} h`;

function FixCard({ p }: { p: ProjectRow }) {
  const owner = p.details?.owner;
  const customer = p.customer ?? "Unknown customer";
  const project = stripCustomer(p.project, p.customer) ?? p.account_key;
  const subject = encodeURIComponent(`Mirres: ${customer} · ${project}`);
  const url = p.details?.contract_url;
  return (
    <li className="mirres-fix-card">
      <div className="mirres-fix-title">
        <ContractMissingIcon size={20} />
        <span>
          <strong>{customer}</strong> · {project}
        </span>
      </div>
      <p>{warningHelp(p.warning) ?? p.warning}</p>
      {(owner?.email || url) && (
        <div className="mirres-fix-actions">
          {owner?.email && (
            <a className="action-btn" href={`mailto:${owner.email}?subject=${subject}`}>
              Ask {owner.name.split(" ")[0]}
            </a>
          )}
          {url && (
            <a href={url} target="_blank" rel="noreferrer">
              Open contract
            </a>
          )}
        </div>
      )}
      <span className="mirres-fix-meta">
        {hrs(p.seconds)} · {p.tickets.join(", ")}
      </span>
    </li>
  );
}

/** Projects whose contract Mirres can't use, each with who to ask. */
export function MirresFixList({ days }: { days: MirresDay[] }) {
  const need = projectsFrom(days).filter((p) => statusKind(p) === "missing");
  return (
    <section className="reg-section" aria-labelledby="mirres-fix-h">
      <h2 id="mirres-fix-h">
        Fix in Mirres {need.length > 0 && <span className="mirres-count">{need.length}</span>}
      </h2>
      {need.length === 0 ? (
        <div className="mirres-clear">
          <AllClearSpot width={96} />
          <span>Nothing to fix — every project has a contract in Mirres.</span>
        </div>
      ) : (
        <ul className="mirres-fix-grid">
          {need.map((p) => (
            <FixCard key={p.account_key} p={p} />
          ))}
        </ul>
      )}
    </section>
  );
}
