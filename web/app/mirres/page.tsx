// Mirres pane: where the fetched days' hours went, by customer, and what
// Mirres needs fixed. Read-only apart from the fetch action; the daemon owns
// the data.

import Link from "next/link";
import { ChevronLeft } from "lucide-react";

import { MirresAutoFetch } from "@/components/MirresAutoFetch";
import { MirresDays } from "@/components/MirresDays";
import { MirresFetch } from "@/components/MirresFetch";
import { MirresFixList } from "@/components/MirresFixList";
import { MirresLedger } from "@/components/MirresLedger";
import { LedgerEmpty } from "@/components/mirres-spots";
import { mirresOverview } from "@/lib/daemonTempoLines";
import { formatDayHeading, todayISO } from "@/lib/format";
import type { MirresDay } from "@/lib/tempo_line_contract";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Mirres · worklog",
};

interface Props {
  searchParams: Promise<{ from?: string }>;
}

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
            Where your logged hours went, by customer — and what Mirres needs fixed.
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
        <Empty />
      ) : (
        <>
          {/* Newest 7 stored days only: caps Jira/Mirres calls per visit. */}
          <MirresAutoFetch days={days.slice(0, 7).map((d) => d.day)} />
          <MirresLedger days={days} />
          <MirresFixList days={days} />
          <MirresDays days={days} />
        </>
      )}
    </main>
  );
}

function Empty() {
  return (
    <section className="reg-section mirres-empty">
      <LedgerEmpty width={160} />
      <p className="mirres-empty-title">Nothing fetched from Mirres yet.</p>
      <p className="mirres-empty-hint">
        Pick a day above and press Fetch from Mirres — worklog looks up each ticket&apos;s account
        and asks Mirres who the customer is.
      </p>
    </section>
  );
}
