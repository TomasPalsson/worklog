// Mirres spot illustrations. Same hand as icons.tsx: 2 round stroke (heavier than icons: spots read thin otherwise),
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

/** Empty state: a Mirres tag hanging from a peg on the day strip; the rows hold no tags yet. */
export function LedgerEmpty({ width = 160, ...p }: SpotProps) {
  return (
    <Spot width={width} vb={[160, 120]} {...p}>
      <path d="M40 62V16a4 4 0 0 1 4-4h12" {...NS} />
      <g transform="rotate(-4 58 36)">
        <path d="M58 13V33.5" {...NS} />
        <path d="M46 36 56 26h56v20H56z" {...WASH} />
        <path d="M46 36 56 26h56v20H56z" {...NS} />
        <circle cx="58" cy="36" r="2.5" {...NS} />
        <path d="M74 33h26M74 40h14" {...NS} />
      </g>
      <path d="M28 76V66a4 4 0 0 1 4-4h96a4 4 0 0 1 4 4v10z" {...WASH} />
      <rect x="28" y="62" width="104" height="52" rx="4" {...NS} />
      <path d="M28 76h104M28 88h104M28 100.5h104" {...NS} />
      <path d="M36 76v-6M52 76v-6M68 76v-6M84 76v-6M100 76v-6M116 76v-6M44 76v-3M60 76v-3M76 76v-3M92 76v-3M108 76v-3M124 76v-3" {...NS} />
      <path d="M36 82l3.5-3.5h13v7h-13zM36 94.5l3.5-3.5h8v7h-8zM36 107l3.5-3.5h13v7h-13z" {...NS} />
    </Spot>
  );
}

/** Nothing needs fixing: the tag, checked, lying on a settled day strip. */
export function AllClearSpot({ width = 96, ...p }: SpotProps) {
  return (
    <Spot width={width} vb={[96, 72]} {...p}>
      <path d="M8 16V9a5 5 0 0 1 5-5h70a5 5 0 0 1 5 5v7z" {...WASH} />
      <rect x="8" y="4" width="80" height="64" rx="5" {...NS} />
      <path d="M8 16h80" {...NS} />
      <path d="M16 16v-6M32 16v-6M48 16v-6M64 16v-6M80 16v-6M24 16v-3M40 16v-3M56 16v-3M72 16v-3" {...NS} />
      <g transform="rotate(-3 52 35)">
        <path d="M18 35 26 24h52v22H26z" {...WASH} />
        <path d="M18 35 26 24h52v22H26z" {...NS} />
        <circle cx="25" cy="35" r="1.8" {...NS} />
        <path d="M43 35.5l6 6 11-12" {...NS} />
      </g>
      <path d="M18 54h44M18 60h28" {...NS} />
    </Spot>
  );
}
