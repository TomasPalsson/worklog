// Unbilled-work spot illustrations. Same hand as icons.tsx: 2 round stroke (heavier than icons: spots read thin otherwise),
// currentColor, one soft wash. Stroke stays 1.6px at any size.

import type { ReactNode, SVGProps } from "react";

type SpotProps = Omit<SVGProps<SVGSVGElement>, "children"> & { width?: number };

const NS = { vectorEffect: "non-scaling-stroke" } as const;
const WASH = { fill: "currentColor", stroke: "none", opacity: 0.24 } as const;

function Spot({
  width,
  vb,
  children,
  ...rest
}: SpotProps & { width: number; vb: [number, number]; children: ReactNode }) {
  return (
    <svg
      width={width}
      height={(width * vb[1]) / vb[0]}
      viewBox={`0 0 ${vb[0]} ${vb[1]}`}
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {children}
    </svg>
  );
}

/** Empty state: a strip of ticked day receipts (weekday and weekend marks, a cycle-start tab) with a round "billed" stamp resting on one; the whole cycle is settled. */
export function AllSettledSpot({ width = 160, ...p }: SpotProps) {
  return (
    <Spot width={width} vb={[160, 120]} {...p}>
      <circle cx="92" cy="40" r="22" {...WASH} />
      <g transform="rotate(-8 92 40)">
        <circle cx="92" cy="40" r="22" {...NS} />
        <circle cx="92" cy="40" r="16" {...NS} />
        <path d="M83 40l6 6 11-12" {...NS} />
      </g>
      <path d="M15 64v-5h10v5" {...NS} />
      <path d="M10 100V68a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v32l-5 5l-5 -5l-5 5l-5 -5zM34 100V68a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v32l-5 5l-5 -5l-5 5l-5 -5zM58 100V68a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v32l-5 5l-5 -5l-5 5l-5 -5zM82 100V68a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v32l-5 5l-5 -5l-5 5l-5 -5zM106 100V68a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v32l-5 5l-5 -5l-5 5l-5 -5zM130 100V68a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v32l-5 5l-5 -5l-5 5l-5 -5z" {...NS} />
      <path d="M20 68v6M44 68v6M68 68v6M92 68v6M116 68v3M140 68v3" {...NS} />
      <path d="M16 88l3 3 6-6M40 88l3 3 6-6M64 88l3 3 6-6M88 88l3 3 6-6M112 88l3 3 6-6M136 88l3 3 6-6" {...NS} />
    </Spot>
  );
}

/** Error state: a ledger strip torn in two with its link wire broken between the halves; the list cannot reach its source, nothing is lost. */
export function DaemonDownSpot({ width = 96, ...p }: SpotProps) {
  return (
    <Spot width={width} vb={[96, 72]} {...p}>
      <path d="M34 18H8a4 4 0 0 0-4 4v28a4 4 0 0 0 4 4h26l4-9-4-9 4-9-4-9z" {...NS} />
      <path d="M10 30h14M10 42h10" {...NS} />
      <path d="M62 18h26a4 4 0 0 1 4 4v28a4 4 0 0 1-4 4H62l-4-9 4-9-4-9 4-9z" {...WASH} />
      <path d="M62 18h26a4 4 0 0 1 4 4v28a4 4 0 0 1-4 4H62l-4-9 4-9-4-9 4-9z" {...NS} />
      <path d="M70 30h14M70 42h10" {...NS} />
      <path d="M38 36c4 0 3 6 7 6M58 36c-4 0-3-6-7-6" {...NS} />
    </Spot>
  );
}
