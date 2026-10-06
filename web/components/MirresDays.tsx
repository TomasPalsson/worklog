import Link from "next/link";
import { MirresFetch } from "./MirresFetch";
import { daySummary, formatFetchedAt, shortDay } from "@/lib/mirresOverview";
import type { MirresDay } from "@/lib/tempo_line_contract";

export function MirresDays({ days }: { days: MirresDay[] }) {
  return (
    <section className="reg-section" aria-labelledby="mirres-days-h">
      <h2 id="mirres-days-h">Fetched days</h2>
      <ul className="mirres-days">
        {days.map((d) => {
          const s = daySummary(d);
          return (
            <li key={d.day}>
              <Link href={`/${d.day}`}>{shortDay(d.day)}</Link>
              <span>
                {s.matched === s.total ? `all ${s.total} tickets matched` : `${s.matched} of ${s.total} tickets matched`}
              </span>
              <span>{s.billablePercent === null ? "—" : `${s.billablePercent}% billable`}</span>
              <time dateTime={d.fetched_at}>{formatFetchedAt(d.fetched_at)}</time>
              <MirresFetch fixedDay={d.day} />
            </li>
          );
        })}
      </ul>
    </section>
  );
}
