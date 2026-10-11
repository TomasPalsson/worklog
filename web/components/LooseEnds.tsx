import Link from "next/link";
import { formatDuration } from "@/lib/format";
import type { LooseEndMissing, LooseEnds as LooseEndsData } from "@/lib/time_leaks_contract";

const MISSING_LABEL: Record<LooseEndMissing, string> = { tempo: "Tempo id", export: "export date" };

export function LooseEnds({ data }: { data: LooseEndsData | null }) {
  if (!data) {
    return (
      <p role="alert" className="review-sec-error">
        The loose-ends list could not be loaded.
      </p>
    );
  }
  return (
    <>
      <p>
        Billing cycle {data.cycle_start} to {data.cycle_end}
      </p>
      {data.items.length === 0 ? (
        <p>Nothing is loose in this cycle.</p>
      ) : (
        <ul>
          {data.items.map((e) => (
            <li key={e.block_id}>
              <Link href={`/${e.day}`}>{e.day}</Link> {e.jira_issue} {formatDuration(e.seconds)} — missing{" "}
              {e.missing.map((m) => MISSING_LABEL[m]).join(" and ")}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
