import { notFound } from "next/navigation";
import { DaemonError } from "@/lib/daemon";
import { getLogged } from "@/lib/daemonLogged";
import { LoggedDay } from "@/components/LoggedDay";
import { LoggedFetch } from "@/components/LoggedFetch";
import { LoggedHeader } from "@/components/LoggedHeader";

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

export const dynamic = "force-dynamic";

export default async function LoggedDayPage({ params }: { params: Promise<{ day: string }> }) {
  const { day } = await params;
  if (!DAY_RE.test(day)) notFound();

  let range: Awaited<ReturnType<typeof getLogged>>;
  try {
    range = await getLogged(day, day);
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
        view="day"
        id={day}
        range={range}
        fetch={<LoggedFetch from={day} to={day} pulledAt={range.pulled_at} />}
      />
      <LoggedDay range={range} />
    </>
  );
}
