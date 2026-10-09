// Inked rubber stamp. Full size: EXPORTED on a circular textPath, the date
// between two rules, two stars. Mini (size < 48): just DONE. Real <text>
// throughout; a turbulence filter roughens and speckles the ink.

import { useId } from "react";

export function stampLabel(date: string, mini: boolean): string {
  return mini ? "Line done" : `Exported on ${date}`;
}

// Roughen the edge (displacement) and punch speckle holes (noise alpha mask).
function InkFilter({ id }: { id: string }) {
  return (
    <filter id={id} x="-5%" y="-5%" width="110%" height="110%">
      <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="4" result="noise" />
      <feDisplacementMap in="SourceGraphic" in2="noise" scale="1.2" result="disp" />
      <feColorMatrix in="noise" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  1 0 0 0 0" result="a" />
      <feComponentTransfer in="a" result="mask">
        <feFuncA type="linear" slope="4" intercept="-0.7" />
      </feComponentTransfer>
      <feComposite in="disp" in2="mask" operator="in" />
    </filter>
  );
}

export function ExportedStamp({ date, size = 96 }: { date: string; size?: number }) {
  const uid = useId();
  const pathId = `${uid}-arc`;
  const filterId = `${uid}-ink`;
  const mini = size < 48;
  // own sizes inline: .art-label's 10px would beat the fontSize attributes
  const ink = (fontSize: number) => ({ fill: "currentColor", fontSize, fontFamily: "var(--font-mono)" });
  return (
    <span className={`art-export-stamp-shake${mini ? " is-mini" : ""}`}>
      <svg
        className="art-export-stamp"
        width={size}
        height={size}
        viewBox="0 0 100 100"
        role="img"
        aria-label={stampLabel(date, mini)}
        style={{ color: "var(--sage-ink)" }}
      >
        <defs>
          <InkFilter id={filterId} />
          <path id={pathId} d="M 20 50 A 30 30 0 0 1 80 50" />
        </defs>
        <g
          filter={`url(#${filterId})`}
          fill="currentColor"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          opacity="0.85"
        >
          <circle cx="50" cy="50" r="46" fill="none" strokeWidth={mini ? 5 : 2.4} />
          {mini ? (
            <text x="50" y="62" textAnchor="middle" fontWeight="700" stroke="none" style={ink(30)}>
              DONE
            </text>
          ) : (
            <>
              <circle cx="50" cy="50" r="41" fill="none" strokeWidth="1.2" />
              <text fontWeight="700" letterSpacing="2" textAnchor="middle" stroke="none" style={ink(11)}>
                <textPath href={`#${pathId}`} startOffset="50%">EXPORTED</textPath>
              </text>
              <line x1="16" y1="44" x2="84" y2="44" strokeWidth="1.2" />
              <line x1="16" y1="60" x2="84" y2="60" strokeWidth="1.2" />
              <text x="50" y="55" textAnchor="middle" stroke="none" style={ink(9)}>
                {date}
              </text>
              <text x="36" y="78" textAnchor="middle" fontSize="9" stroke="none">★</text>
              <text x="64" y="78" textAnchor="middle" fontSize="9" stroke="none">★</text>
            </>
          )}
        </g>
      </svg>
    </span>
  );
}
