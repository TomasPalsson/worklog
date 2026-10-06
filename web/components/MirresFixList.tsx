"use client";

import { useState } from "react";
import { ContractMissingIcon } from "./mirres-art";
import { AllClearSpot } from "./mirres-spots";
import { hrs, projectsFrom, sharedHelp, stripCustomer, warningHelp } from "@/lib/mirresOverview";
import { statusKind } from "@/lib/mirresStatus";
import type { ProjectRow } from "@/lib/mirresOverview";
import type { MirresDay } from "@/lib/tempo_line_contract";

function CopyMessage({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setDone(true);
      setTimeout(() => setDone(false), 2000);
    } catch {
      // clipboard unavailable: the button simply stays as it was
    }
  }
  return (
    <>
      <button type="button" className="action-btn" title={text} onClick={copy}>
        Copy message
      </button>
      <span aria-live="polite">{done ? "Copied" : ""}</span>
    </>
  );
}

function Action({ p, customer, project }: { p: ProjectRow; customer: string; project: string }) {
  const owner = p.details?.owner;
  const url = p.details?.contract_url;
  if (owner?.email) {
    const subject = encodeURIComponent(`Mirres: ${customer} · ${project}`);
    return (
      <a className="action-btn" href={`mailto:${owner.email}?subject=${subject}`}>
        Email {owner.name.split(" ")[0]}
      </a>
    );
  }
  if (url) {
    return (
      <a className="action-btn" href={url} target="_blank" rel="noreferrer">
        Open contract
      </a>
    );
  }
  return (
    <CopyMessage
      text={`Hi — ${customer} · ${project} (${p.account_key}) has no contract in Mirres. Could you add it? Thanks`}
    />
  );
}

function FixRow({ p, help }: { p: ProjectRow; help: boolean }) {
  const customer = p.customer ?? "Unknown customer";
  const project = stripCustomer(p.project, p.customer) ?? p.account_key;
  return (
    <li className="mirres-fix-row">
      <ContractMissingIcon size={16} />
      <span className="mirres-fix-name">
        <strong>{customer}</strong> · {project}
        {help && <span className="mirres-fix-help">{warningHelp(p.warning) ?? p.warning}</span>}
      </span>
      <span className="mirres-fix-hours">{hrs(p.seconds)}</span>
      <span className="mirres-fix-meta">{p.tickets.join(", ")}</span>
      <span className="mirres-fix-action">
        <Action p={p} customer={customer} project={project} />
      </span>
    </li>
  );
}

/** Projects whose contract Mirres can't use, each with who to ask. */
export function MirresFixList({ days }: { days: MirresDay[] }) {
  const need = projectsFrom(days).filter((p) => statusKind(p) === "missing");
  const shared = sharedHelp(need.map((p) => p.warning), need.length);
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
        <>
          {shared && <p className="mirres-fix-intro">{shared}</p>}
          <ul className="mirres-fix-list">
            {need.map((p) => (
              <FixRow key={p.account_key} p={p} help={!shared} />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
