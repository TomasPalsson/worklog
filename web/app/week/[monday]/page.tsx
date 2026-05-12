import { notFound } from "next/navigation";
import { DaemonError, loadDaySummary } from "@/lib/daemon";
import {
  formatTotalHours,
  mondayOf,
  weekDays,
} from "@/lib/format";
import { WeekHeader } from "@/components/WeekHeader";
import { WeekGrid } from "@/components/WeekGrid";

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

// Reads live data on every request — same as the day page.
export const dynamic = "force-dynamic";

export default async function WeekPage({
  params,
}: {
  params: Promise<{ monday: string }>;
}) {
  const { monday: param } = await params;
  if (!DAY_RE.test(param)) notFound();

  // We accept any ISO day in the URL and normalise to its Monday so
  // bookmarks like /week/2026-05-15 still resolve to a real week.
  const monday = mondayOf(param);
  const days = weekDays(monday);

  // Fan out — 7 small daemon hits in parallel beats one bespoke
  // /weeks/:monday endpoint for now. Local loopback HTTP is sub-ms.
  let summaries: Awaited<ReturnType<typeof loadDaySummary>>[];
  try {
    summaries = await Promise.all(days.map((d) => loadDaySummary(d)));
  } catch (e) {
    if (e instanceof DaemonError) {
      throw new Error(
        `Can't reach the worklog daemon — start it on the host with ` +
          `\`worklog daemon\` or \`worklog daemon install\`. (${e.message})`,
      );
    }
    throw e;
  }

  const dayCols = days.map((d, i) => ({
    day: d,
    blocks: summaries[i].blocks,
    totalSeconds: summaries[i].total_seconds,
  }));

  const workSeconds = dayCols.reduce(
    (acc, c) =>
      acc +
      c.blocks
        .filter((b) => !b.is_personal)
        .reduce((s, b) => s + b.duration_seconds, 0),
    0,
  );
  const personalSeconds = dayCols.reduce(
    (acc, c) =>
      acc +
      c.blocks
        .filter((b) => b.is_personal)
        .reduce((s, b) => s + b.duration_seconds, 0),
    0,
  );
  const workBlocks = dayCols.reduce(
    (acc, c) => acc + c.blocks.filter((b) => !b.is_personal).length,
    0,
  );
  const personalSummary =
    personalSeconds > 0
      ? `${formatTotalHours(personalSeconds)} personal`
      : undefined;

  return (
    <>
      <WeekHeader
        monday={monday}
        workSeconds={workSeconds}
        workBlocks={workBlocks}
        personalSummary={personalSummary}
      />
      <WeekGrid days={dayCols} />
    </>
  );
}
