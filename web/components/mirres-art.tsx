// Mirres status art: the luggage-tag anchor (see MirresIcon in icons.tsx)
// plus one mark per meaning. Same hand as icons.tsx: 24 grid, 1.6 stroke,
// one currentColor wash. Decorative: aria-hidden; the control carries the
// label. The parent sets the ink via `color`.

import type { ReactNode, SVGProps } from "react";

type IconProps = Omit<SVGProps<SVGSVGElement>, "children"> & { size?: number };

function Svg({ size = 22, children, ...rest }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
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

const WASH = { fill: "currentColor", stroke: "none", opacity: 0.24 } as const;


// Same pointed-left tag as MirresIcon, with a shorter nose so the body has
// room for an 8-unit mark (x 10.5-18.5, y 8-16).
const TAG = "M4.5 12 8.5 5.5H20v13H8.5z";

function Tag() {
  return (
    <>
      <path d={TAG} {...WASH} />
      <path d={TAG} />
      <circle cx="6.9" cy="12" r="0.8" />
    </>
  );
}

/** Billable (Reikningshæft): the Mirres tag holding one coin. */
export function BillableIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <Tag />
      {/* two stacked coins */}
      <ellipse cx="14.5" cy="9.6" rx="3.6" ry="1.5" />
      <path d="M10.9 9.6v4.8a3.6 1.5 0 0 0 7.2 0V9.6M10.9 12a3.6 1.5 0 0 0 7.2 0" />
    </Svg>
  );
}

/** Included: hours covered by the contract; the tag holds a contract sheet. */
export function IncludedIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <Tag />
      <path d="M12 7.8h3.8l2.2 2.2v6.6a.9.9 0 0 1-.9.9H12a.9.9 0 0 1-.9-.9V8.7a.9.9 0 0 1 .9-.9z" />
      <path d="M15.8 7.8V10H18" />
    </Svg>
  );
}

/** Fixed price (Fast verð / Tilboð): a closed padlock on the tag, so not billed per hour. */
export function FixedPriceIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <Tag />
      <rect x="11" y="11.4" width="7" height="5.6" rx="1.4" />
      <path d="M12.5 11.4V9.7a2 2 0 0 1 4 0v1.7" />
    </Svg>
  );
}

/** Internal: Apró's own project; the tag holds a house (roof and door). */
export function InternalIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <Tag />
      <path d="M10.4 12.6 14.5 8.4l4.1 4.2" />
      <path d="M11.8 11.4v6h5.4v-6" />
      <path d="M13.8 17.4v-2.6h1.4v2.6" />
    </Svg>
  );
}

/** Contract missing (Samning vantar): a document with a torn foot and a "!". */
export function ContractMissingIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M6 3h8.5L19 7.5V21l-2.2-1.4-2.1 1.4-2.2-1.4-2.2 1.4-2.1-1.4L6 21z" {...WASH} />
      <path d="M6 3h8.5L19 7.5V21l-2.2-1.4-2.1 1.4-2.2-1.4-2.2 1.4-2.1-1.4L6 21z" />
      <path d="M14.5 3v4.5H19" />
      <path d="M12.5 9.8v4.6" />
      <path d="M12.5 17.1h.01" strokeWidth={2.4} />
    </Svg>
  );
}

/** Info: a circled "i" that opens the details popover. */
export function InfoIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="8" {...WASH} opacity={0.16} />
      <circle cx="12" cy="12" r="8" />
      <path d="M12 8.2h.01" strokeWidth={2.2} />
      <path d="M12 11.3v4.6" />
    </Svg>
  );
}
