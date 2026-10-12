import { Unbilled } from "@/components/Unbilled";
import { fetchLooseEnds } from "@/lib/daemonLooseEnds";
import { todayISO } from "@/lib/format";

export const metadata = {
  title: "Unbilled work · worklog",
};

export const dynamic = "force-dynamic";

export default async function UnbilledPage() {
  const data = await fetchLooseEnds().catch((err) => {
    console.error("Unbilled work: fetch failed", err);
    return null;
  });
  return <Unbilled data={data} today={todayISO()} hourNow={new Date().getHours()} />;
}
