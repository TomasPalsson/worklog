// Mirres pane: what Mirres says about the stored ticket lines, per project
// and per fetched day, plus a way to fetch a day again. Read-only apart from
// the fetch action; the daemon owns the data.

import Link from "next/link";
import { ChevronLeft, TriangleAlert } from "lucide-react";

import { MirresFetch } from "@/components/MirresFetch";
import { mirresOverview } from "@/lib/daemonTempoLines";
import { formatDayHeading, formatTotalHours, todayISO } from "@/lib/format";
import { BillingDetails } from "@/components/BillingDetails";
import {
  daySummary,
  formatFetchedAt,
  projectsFrom,
  statusReason,
  stripCustomer,
  totals,
  warningHelp,
} from "@/lib/mirresOverview";
import { BILLING_HINT, BILLING_LABEL } from "@/lib/tempo_line_contract";
import type { ProjectRow } from "@/lib/mirresOverview";
import type { MirresDay } from "@/lib/tempo_line_contract";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Mirres · worklog",
};

interface Props {
  searchParams: Promise<{ from?: string }>;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export default async function MirresPage({ searchParams }: Props) {
  const { from } = await searchParams;
  const back = /^\d{4}-\d{2}-\d{2}$/.test(from ?? "") ? (from as string) : todayISO();

  let days: MirresDay[] = [];
  let loadError: string | null = null;
  try {
    days = await mirresOverview();
  } catch (e) {
    loadError = (e as Error).message || "unknown error";
  }

  return (
    <main className="reg-page mirres-page">
      <header className="reg-page-header">
        <div>
          <Link href={`/${back}`} className="reg-back" data-tip="Back to the day view">
            <ChevronLeft size={14} strokeWidth={1.75} />
            {formatDayHeading(back)}
          </Link>
          <h1>Mirres</h1>
          <p className="reg-lede">
            Which project, type and billable status Mirres reports for your ticket lines.
          </p>
        </div>
        <MirresFetch day={back} />
      </header>

      {loadError ? (
        <div role="alert">
          <p className="export-error">Couldn&apos;t load Mirres data — {loadError}</p>
          <p className="export-hint">Is the worklog daemon running?</p>
        </div>
      ) : days.length === 0 ? (
        <section className="reg-section">
          <p>Nothing fetched from Mirres yet.</p>
          <p className="export-hint">
            Pick a day above and press Fetch from Mirres — worklog looks up each ticket&apos;s Tempo
            account and asks Mirres which project it belongs to.
          </p>
        </section>
      ) : (
        <Overview days={days} />
      )}
    </main>
  );
}

function Overview({ days }: { days: MirresDay[] }) {
  const t = totals(days);
  const projects = projectsFrom(days);
  return (
    <>
      <p className="mirres-summary">
        {t.billablePercent !== null && (
          <>
            <strong className="mirres-figure">{t.billablePercent}%</strong> billable ·{" "}
          </>
        )}
        <strong className="mirres-figure">{formatTotalHours(t.seconds)}</strong> logged ·{" "}
        {plural(t.projects, "project")} · {plural(t.tickets, "ticket")} ·{" "}
        {plural(t.days, "day")} fetched
        {t.attention > 0 && " · "}
        {t.attention > 0 && (
          <a className="mirres-attention" href="#mirres-attention">
            <TriangleAlert size={12} aria-hidden="true" />
            {t.attention} need{t.attention === 1 ? "s" : ""} attention
          </a>
        )}
      </p>

      <section className="reg-section">
        <h2>Projects</h2>
        <ProjectsTable projects={projects} />
      </section>

      <section className="reg-section">
        <h2>Days fetched</h2>
        <DaysList days={days} />
      </section>
    </>
  );
}

function ProjectsTable({ projects }: { projects: ProjectRow[] }) {
  const firstAttention = projects.findIndex((p) => p.warning);
  return (
    <div className="mirres-table-wrap">
      <table className="mirres-table">
        <caption className="mirres-sr">Projects Mirres reports for fetched ticket lines</caption>
        <thead>
          <tr>
            <th scope="col">Customer</th>
            <th scope="col">Project</th>
            <th scope="col" className="num">Hours</th>
            <th scope="col">Type</th>
            <th scope="col">Status</th>
            <th scope="col">Tickets</th>
          </tr>
        </thead>
        <tbody>
          {projects.map((p, i) => (
            <ProjectTr key={p.account_key} p={p} id={i === firstAttention ? "mirres-attention" : undefined} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ProjectTr({ p, id }: { p: ProjectRow; id?: string }) {
  const reason = statusReason(p.class, p.project_type, p.warning);
  const help = warningHelp(p.warning);
  return (
    <tr id={id} data-attention={p.warning ? "" : undefined}>
      <td className="mirres-customer">{p.customer ?? "—"}</td>
      <td>
        <div>{stripCustomer(p.project, p.customer) ?? "—"}</div>
        <div className="mirres-key">{p.account_key}</div>
        <BillingDetails details={p.details} className="mirres-details" />
        {p.warning && (
          <div className="mirres-warning">
            <TriangleAlert size={12} aria-hidden="true" />
            {p.warning}
          </div>
        )}
        {help && <div className="mirres-help">{help}</div>}
      </td>
      <td className="num">{formatTotalHours(p.seconds)}</td>
      <td>{p.project_type ?? "—"}</td>
      <td>
        <span className={`billing-pill-tag ${p.class}`} title={BILLING_HINT[p.class]}>
          {BILLING_LABEL[p.class]}
        </span>
        {reason && <div className="mirres-reason">{reason}</div>}
      </td>
      <td className="mirres-tickets">{p.tickets.join(", ")}</td>
    </tr>
  );
}

function DaysList({ days }: { days: MirresDay[] }) {
  return (
        <ul className="mirres-days">
          {days.map((d) => {
            const s = daySummary(d);
            return (
              <li key={d.day}>
                <Link href={`/${d.day}`}>{formatDayHeading(d.day)}</Link>
                <span>{s.matched} of {s.total} lines matched</span>
                <span>{s.billablePercent === null ? "—" : `${s.billablePercent}% billable`}</span>
                <time dateTime={d.fetched_at}>{formatFetchedAt(d.fetched_at)}</time>
                <MirresFetch fixedDay={d.day} />
              </li>
            );
          })}
        </ul>
  );
}
