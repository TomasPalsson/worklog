import Link from "next/link";
import { ScrollText } from "lucide-react";

/** The block card's way into its full Details page. */
export function BlockDetailsLink({ day, blockId }: { day: string; blockId: number }) {
  return (
    <Link
      href={`/${day}/block/${blockId}`}
      className="events-disclosure events-details-link"
      title="Every prompt, tool call, command and commit in this block"
    >
      <ScrollText className="disclosure-chev" aria-hidden="true" />
      Details
    </Link>
  );
}
