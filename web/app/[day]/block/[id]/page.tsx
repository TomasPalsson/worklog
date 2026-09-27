import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { DaemonError, loadDaySummary } from "@/lib/daemon";
import { blockDetails } from "@/lib/daemonDetails";
import { formatDayHeading, formatDuration, formatRange } from "@/lib/format";
import type { Block } from "@/lib/types";
import { BlockDetails } from "@/components/BlockDetails";

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;

// Every request reads live DB state — never prerender at build time.
export const dynamic = "force-dynamic";

function BlockHeader({ day, block }: { day: string; block: Block | undefined }) {
  const folder = block?.project ?? block?.project_path?.split("/").filter(Boolean).pop();
  return (
    <header className="bd-header">
      <Link href={`/${day}`} className="bd-back">
        <ArrowLeft width={14} height={14} aria-hidden="true" />
        {formatDayHeading(day)}
      </Link>
      <h1 className="bd-title">{block?.description?.trim() || "Untitled block"}</h1>
      {block && (
        <p className="bd-meta">
          <span>{formatRange(block.started_at, block.ended_at)}</span>
          <span>{formatDuration(block.duration_seconds)}</span>
          {folder && <span className="bd-meta-chip">{folder}</span>}
          {block.jira_issue && <span className="bd-meta-chip">{block.jira_issue}</span>}
          {block.is_personal && <span className="bd-meta-chip">personal</span>}
        </p>
      )}
    </header>
  );
}

export default async function BlockDetailPage({ params }: { params: Promise<{ day: string; id: string }> }) {
  const { day, id } = await params;
  if (!DAY_RE.test(day)) notFound();
  const blockId = Number(id);
  if (!Number.isInteger(blockId)) notFound();

  try {
    const [rows, summary] = await Promise.all([
      blockDetails(blockId),
      loadDaySummary(day).catch(() => null),
    ]);
    const block = summary?.blocks.find((b) => b.id === blockId);
    return (
      <div className="bd-page">
        <BlockHeader day={day} block={block} />
        <BlockDetails rows={rows} />
      </div>
    );
  } catch (e) {
    // Degrade inline rather than failing the page, matching the day view.
    const message = e instanceof DaemonError ? e.message : "unknown error";
    return (
      <div className="bd-page">
        <BlockHeader day={day} block={undefined} />
        <p className="bd-empty">Can&apos;t load this block — {message}.</p>
      </div>
    );
  }
}
