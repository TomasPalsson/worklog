import { notFound } from "next/navigation";
import { DaemonError } from "@/lib/daemon";
import { getLogged } from "@/lib/daemonLogged";
import { mondayOf, shiftDay } from "@/lib/format";
import { LoggedFetch } from "@/components/LoggedFetch";
import { LoggedHeader } from "@/components/LoggedHeader";
import { LoggedWeek } from "@/components/LoggedWeek";

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

export const dynamic = "force-dynamic";

export default async function LoggedWeekPage({ params }: { params: Promise<{ monday: string }> }) {
  const { monday: param } = await params;
  if (!DAY_RE.test(param)) notFound();
  const monday = mondayOf(param);
  const to = shiftDay(monday, 6);

  let range: Awaited<ReturnType<typeof getLogged>>;
  try {
    range = await getLogged(monday, to);
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
    <>
      <LoggedHeader
        view="week"
        id={monday}
        range={range}
        fetch={<LoggedFetch from={monday} to={to} pulledAt={range.pulled_at} />}
      />
      <LoggedWeek range={range} />
    </>
  );
}
