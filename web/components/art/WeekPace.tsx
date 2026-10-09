import type { CSSProperties } from "react";
import type { CloseoutDay } from "@/lib/types";

const h1 = (s: number) => (s / 3600).toFixed(1);
const pct = (v: number, max: number) => (max > 0 ? Math.min(v / max, 1) * 100 : 0);

/** Weekly bullet chart: Tempo (solid) and worked (underlay) against required, with a pace tick at today. */
export function WeekPace({ days, today, worked }: { days: CloseoutDay[]; today: string; worked: number }) {
  const req = days.reduce((t, d) => t + (d.required_seconds ?? 0), 0);
  if (req <= 0) return null;
  const tempo = days.reduce((t, d) => t + d.tempo_seconds, 0);
  const pace = days.filter((d) => d.day <= today).reduce((t, d) => t + (d.required_seconds ?? 0), 0);
  let run = 0;
  const seps = days.slice(0, -1).map((d) => (run += d.required_seconds ?? 0));
  const diff = pace - tempo;
  const delta = Math.abs(diff) < 900 ? null : diff > 0 ? "behind" : "ahead";
  const left = (v: number) => ({ left: `${pct(v, req)}%` }) as CSSProperties;
  const bar = (v: number): CSSProperties => ({ width: `${pct(v, req)}%` });
  const label =
    `Week pace: ${h1(tempo)} of ${h1(req)} hours in Tempo, ${h1(worked)} worked, ` +
    `${h1(pace)} required through today` +
    (delta ? `, ${h1(Math.abs(diff))} hours ${delta}` : "");
  return (
    <div className="art-meters-pace" role="img" aria-label={label} data-testid="week-pace">
      <div className="art-meters-pace-row" aria-hidden="true">
        <div className="art-meters-pace-chart">
          <div className="art-meters-track">
            <div className="art-meters-bar art-meters-worked art-grow" data-testid="pace-worked" style={bar(worked)} />
            {delta === "behind" && (
              <div
                className="art-meters-bar art-meters-gap art-grow"
                data-testid="pace-gap"
                style={{ ...left(tempo), width: `${pct(pace, req) - pct(tempo, req)}%` }}
              />
            )}
            <div className="art-meters-bar art-meters-tempo art-grow" data-testid="pace-tempo" style={bar(tempo)} />
            {seps.map((s, i) => (
              <span key={i} className="art-meters-sep" style={left(s)} />
            ))}
          </div>
          <span className="art-meters-pace-tick" data-testid="pace-tick" style={left(pace)} />
          <span className="art-meters-today art-label" style={left(pace)}>today</span>
        </div>
        {delta && (
          <span className="art-meters-delta art-label" data-k={delta}>
            {h1(Math.abs(diff))}h {delta}
          </span>
        )}
        <span className="art-meters-total art-label">
          {h1(tempo)} / {h1(req)}h
        </span>
      </div>
    </div>
  );
}
