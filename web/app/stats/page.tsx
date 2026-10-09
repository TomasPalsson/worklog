// Statistics: "Wrapped" for the workday. Every number comes from the daemon's
// GET /stats; this page only lays the story out.

import type { ReactNode } from "react";

import { DayFlow } from "@/components/art/DayFlow";
import { ActivityStream } from "@/components/stats/ActivityStream";
import { CalendarHeatmap } from "@/components/stats/CalendarHeatmap";
import { DailyStack } from "@/components/stats/DailyStack";
import { FunFacts } from "@/components/stats/FunFacts";
import { PeriodReceipt } from "@/components/stats/PeriodReceipt";
import { PoliteBot } from "@/components/stats/PoliteBot";
import { RangeChips, parseRange, rangeDates } from "@/components/stats/RangeChips";
import { RankedBars } from "@/components/stats/RankedBars";
import { SplitDonut } from "@/components/stats/SplitDonut";
import { StatsHero } from "@/components/stats/StatsHero";
import { TicketMetro } from "@/components/stats/TicketMetro";
import { ToolWarehouse } from "@/components/stats/ToolWarehouse";
import { WeekTerrain } from "@/components/stats/WeekTerrain";
import { WorkdayCity } from "@/components/stats/WorkdayCity";
import { WorkdaySpans } from "@/components/stats/WorkdaySpans";
import { getStats } from "@/lib/daemonStats";
import { formatTotalHours, todayISO } from "@/lib/format";
import type { Ranked, StatsReport } from "@/lib/stats_contract";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Statistics · worklog",
};

interface Props {
  searchParams: Promise<{ range?: string }>;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="stats-sec">
      <h2>{title}</h2>
      {children}
    </section>
  );
}

// The daemon reports raw column values; "none" stays as-is so SplitDonut hatches it.
const ESTIMATED_BY: Record<string, string> = { claude_p: "Claude", manual: "you" };
const TICKET_ORIGIN: Record<string, string> = { event: "from activity", auto: "auto-picked", manual: "you" };
const friendly = (rows: Ranked[], names: Record<string, string>) =>
  rows.map((x) => ({ ...x, label: names[x.label] ?? x.label }));

function Story({ r }: { r: StatsReport }) {
  return (
    <>
      <StatsHero report={r} />
      <Section title="Your city">
        <WorkdayCity daily={r.daily} today={r.today} />
      </Section>
      <Section title="Records">
        <FunFacts records={r.records} />
      </Section>
      <Section title="Where the time went">
        <DayFlow
          blocks={r.flow_blocks}
          billing={null}
          defaultOpen
          title={<>Where <b>{formatTotalHours(r.totals.work_seconds)}</b> went</>}
        />
      </Section>
      <Section title="When you work">
        <div className="stats-stack">
          <WeekTerrain grid={r.punchcard} />
          <WorkdaySpans daily={r.daily} />
        </div>
      </Section>
      <Section title="Every day">
        <div className="stats-stack">
          <CalendarHeatmap daily={r.daily} />
          <DailyStack daily={r.daily} />
          <ActivityStream daily={r.daily} />
        </div>
      </Section>
      <Section title="Claude & you">
        <div className="stats-stack">
          <ToolWarehouse
            tools={r.tools}
            helpers={r.helpers}
            totalCalls={r.totals.tool_calls}
            totalHelpers={r.totals.helpers}
          />
          <PoliteBot prompt={r.prompt} />
        </div>
      </Section>
      <Section title="Tools of the trade">
        <div className="stats-two">
          <RankedBars title="Shell commands" unit="count" rows={r.shell} />
          <RankedBars title="Websites" unit="minutes" rows={r.domains} />
          <RankedBars title="Slack" unit="count" rows={r.slack_channels} />
          <RankedBars title="Projects" unit="seconds" rows={r.folders} />
        </div>
      </Section>
      <Section title="Tickets">
        <TicketMetro tickets={r.tickets} from={r.from} to={r.to} />
      </Section>
      <Section title="Housekeeping">
        <div className="stats-three">
          <SplitDonut title="Who set the hours" rows={friendly(r.estimates, ESTIMATED_BY)} unit="count" />
          <SplitDonut title="How tickets got picked" rows={friendly(r.ticket_origin, TICKET_ORIGIN)} unit="count" />
          <SplitDonut
            title="Synced to Tempo"
            rows={[
              { label: "synced", value: r.sync.synced_seconds },
              { label: "not yet", value: r.sync.unsynced_seconds },
            ]}
            unit="seconds"
          />
        </div>
      </Section>
      <Section title="Your receipt">
        <PeriodReceipt report={r} />
      </Section>
    </>
  );
}

export default async function StatsPage({ searchParams }: Props) {
  const range = parseRange((await searchParams).range);
  const { from, to } = rangeDates(range, todayISO());

  let report: StatsReport | null = null;
  let loadError: string | null = null;
  try {
    report = await getStats(from, to);
  } catch (e) {
    loadError = (e as Error).message || "unknown error";
  }

  return (
    <div className="stats-main">
      <header className="stats-header">
        <div>
          <h1>Statistics</h1>
          {report && (
            <p className="stats-sub">
              {report.from} → {report.to} · {report.totals.days_worked} {report.totals.days_worked === 1 ? "day" : "days"} worked
            </p>
          )}
        </div>
        <RangeChips current={range} />
      </header>
      {report ? (
        <Story r={report} />
      ) : (
        <div role="alert">
          <p className="export-error">Couldn&apos;t load statistics — {loadError}</p>
          <p className="export-hint">Is the worklog daemon running?</p>
        </div>
      )}
    </div>
  );
}
