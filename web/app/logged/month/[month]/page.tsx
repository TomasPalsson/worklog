import { notFound } from "next/navigation";
import { DaemonError } from "@/lib/daemon";
import { getLogged } from "@/lib/daemonLogged";
import { monthGrid } from "@/lib/format";
import { LoggedFetch } from "@/components/LoggedFetch";
import { LoggedHeader } from "@/components/LoggedHeader";
import { LoggedMonth } from "@/components/LoggedMonth";

const MONTH_RE = /^\d{4}-\d{2}$/;

export const dynamic = "force-dynamic";

export default async function LoggedMonthPage({ params }: { params: Promise<{ month: string }> }) {
  const { month } = await params;
  if (!MONTH_RE.test(month)) notFound();
  const { from, to } = monthGrid(month);

  let range: Awaited<ReturnType<typeof getLogged>>;
  try {
    range = await getLogged(from, to);
  } catch (e) {
    if (e instanceof DaemonError) {
      throw new Error(
        `Can't reach the worklog daemon — start it on the host with ` +
          `\`worklog daemon\` or \`worklog daemon install\`. (${e.message})`,
      );
    }
    throw e;
  }

  return (
    <div className="logged-page">
      <LoggedHeader view="month" id={month} range={range} />
      <LoggedFetch from={from} to={to} pulledAt={range.pulled_at} />
      <LoggedMonth range={range} month={month} />
    </div>
  );
}
