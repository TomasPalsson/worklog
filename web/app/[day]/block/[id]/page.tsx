import Link from "next/link";
import { notFound } from "next/navigation";
import { DaemonError } from "@/lib/daemon";
import { blockDetails } from "@/lib/daemonDetails";
import { BlockDetails } from "@/components/BlockDetails";

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

// Every request reads live DB state — never prerender at build time.
export const dynamic = "force-dynamic";

export default async function BlockDetailPage({
  params,
}: {
  params: Promise<{ day: string; id: string }>;
}) {
  const { day, id } = await params;
  if (!DAY_RE.test(day)) notFound();
  const blockId = Number(id);
  if (!Number.isInteger(blockId)) notFound();

  const header = (
    <header className="block-details-header">
      <Link href={`/${day}`} className="block-details-back">
        ← back to {day}
      </Link>
      <h1>Block {blockId}</h1>
    </header>
  );

  try {
    const rows = await blockDetails(blockId);
    return (
      <>
        {header}
        <BlockDetails rows={rows} />
      </>
    );
  } catch (e) {
    // Degrade inline rather than failing the page, matching how the day
    // view handles a routedForDay/registry hiccup.
    const message = e instanceof DaemonError ? e.message : "unknown error";
    return (
      <>
        {header}
        <p className="day-empty-work">Can&apos;t load block details — {message}.</p>
      </>
    );
  }
}
