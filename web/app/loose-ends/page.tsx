import { LooseEnds } from "@/components/LooseEnds";
import { fetchLooseEnds } from "@/lib/daemonLooseEnds";

export const metadata = {
  title: "Loose ends · worklog",
};

export const dynamic = "force-dynamic";

export default async function LooseEndsPage() {
  const data = await fetchLooseEnds().catch((err) => {
    console.error("Loose ends: fetch failed", err);
    return null;
  });
  return (
    <main className="reg-page">
      <h1>Loose ends</h1>
      <LooseEnds data={data} />
    </main>
  );
}
