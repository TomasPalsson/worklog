import { useId } from "react";

/** Used share of the allowance, 0..n (can pass 1). `null` = unknown. */
export function gaugeFraction(allowance: number, used: number | null): number | null {
  if (used == null || !(allowance > 0)) return null;
  return Math.max(0, used / allowance);
}

export type GaugeTone = "ok" | "warn" | "over";
export const gaugeTone = (f: number): GaugeTone => (f > 1 ? "over" : f >= 0.75 ? "warn" : "ok");

const h = (n: number) => String(Math.round(n * 10) / 10);

export function gaugeLabel(allowance: number, used: number | null): string {
  if (used == null) return `${h(allowance)} included hours, used unknown`;
  const rest = used > allowance ? `${h(used - allowance)} over` : `${h(allowance - used)} left`;
  return `${h(used)} of ${h(allowance)} included hours used, ${rest}`;
}

/** Fuel gauge for a contract's included hours; the text next to it says the same. */
export function AllowanceGauge({ allowance, used }: { allowance: number; used: number | null }) {
  const clip = useId();
  const f = gaugeFraction(allowance, used);
  const fill = Math.min(f ?? 0, 1);
  const tone = f == null ? null : gaugeTone(f);
  return (
    <span className="art-billing-gauge" role="img" aria-label={gaugeLabel(allowance, used)}>
      <svg className="art-billing-glass" width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
        <clipPath id={clip}>
          <path d="M3 11V9.5L6 6l3 3.5V11Z" />
        </clipPath>
        <rect
          data-testid="gauge-sand"
          x="3"
          y={11 - 5 * fill}
          width="6"
          height={5 * fill}
          fill="currentColor"
          style={{ opacity: "var(--art-wash)" }}
          clipPath={`url(#${clip})`}
        />
        <path
          d="M2 1h8M2 11h8M3 1v1.5L6 6 3 9.5V11M9 1v1.5L6 6l3 3.5V11"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      <span className="art-billing-col" aria-hidden="true">
        <span className="art-billing-track" data-unknown={f == null ? "" : undefined}>
          {tone && (
            <span
              className="art-billing-fill art-grow"
              data-tone={tone}
              style={{ width: `${fill * 100}%`, transformOrigin: "left" }}
            />
          )}
          {tone === "over" && <span className="art-billing-notch" />}
        </span>
        <span className="art-billing-ticks">
          {[0, 25, 50, 75, 100].map((p) => (
            <i key={p} style={{ left: `${p}%` }} />
          ))}
        </span>
        {f == null && <span className="art-label">used unknown</span>}
      </span>
    </span>
  );
}
