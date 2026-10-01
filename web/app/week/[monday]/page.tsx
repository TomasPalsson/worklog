import { notFound } from "next/navigation";
import { DaemonError, loadDaySummary } from "@/lib/daemon";
import {
  formatTotalHours,
  mondayOf,
  weekDays,
} from "@/lib/format";
import { WeekHeader } from "@/components/WeekHeader";
import { WeekGrid } from "@/components/WeekGrid";
import { WeekCloseout } from "@/components/WeekCloseout";
import { closeout } from "@/lib/daemonHub";

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
  let weekCloseout: Awaited<ReturnType<typeof closeout>>;
  try {
    [summaries, weekCloseout] = await Promise.all([
      Promise.all(days.map((d) => loadDaySummary(d))),
      closeout(monday),
    ]);
  } catch (e) {
    if (e instanceof DaemonError) {
      throw new Error(
        `Can't reach the worklog daemon — start it on the host with ` +
          `\`worklog daemon\` or \`worklog daemon install\`. (${e.message})`,
      );
    }
    throw e;
  }

  // Ignored blocks are hidden everywhere: drop them (and their seconds)
  // before any work/personal totals are derived.
  const dayCols = days.map((d, i) => {
    const kept = summaries[i].blocks.filter((b) => !b.ignored_at);
    const ignoredSeconds = summaries[i].blocks
      .filter((b) => b.ignored_at)
      .reduce((s, b) => s + b.duration_seconds, 0);
    return {
      day: d,
      blocks: kept,
      totalSeconds: summaries[i].total_seconds - ignoredSeconds,
    };
  });

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
      <WeekCloseout closeout={weekCloseout} />
      <WeekGrid days={dayCols} />
    </>
  );
}
