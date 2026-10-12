// "Unbilled" page icons, drawn in the same hand as icons.tsx: 24px grid,
// 1.6 stroke, round joins, one soft currentColor wash per icon. Decorative:
// every icon is aria-hidden; the label next to it carries the meaning.

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

/** Unbilled: a sheet whose lower-right corner is still an open, dashed outline. */
export function UnbilledIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M10 20.5H6A2.5 2.5 0 0 1 3.5 18V6A2.5 2.5 0 0 1 6 3.5h9A2.5 2.5 0 0 1 17.5 6v4" />
      <path d="M10 10h7.5v8a2.5 2.5 0 0 1-2.5 2.5h-5z" {...WASH} />
      <path d="M10 20.5h5a2.5 2.5 0 0 0 2.5-2.5v-8" strokeDasharray="2 2.4" />
      <path d="M7 8h6.5M7 11.5h4" />
    </Svg>
  );
}

/** Logged: a timesheet with a washed block of time on it. */
export function StepLoggedIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="4" y="3.5" width="16" height="17" rx="2.5" />
      <path d="M8 7.5h5" />
      <rect x="7.5" y="10.5" width="9" height="6.5" rx="1.5" {...WASH} />
      <rect x="7.5" y="10.5" width="9" height="6.5" rx="1.5" />
      <path d="M12 12.4v2.2l1.5 1" strokeWidth={1.4} />
    </Svg>
  );
}

/** Sent to Tempo: an arrow dropping into an open timesheet tray. */
export function StepTempoIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M3.5 13h17v5a2.5 2.5 0 0 1-2.5 2.5H6A2.5 2.5 0 0 1 3.5 18z" {...WASH} />
      <path d="M3.5 13v5A2.5 2.5 0 0 0 6 20.5h12a2.5 2.5 0 0 0 2.5-2.5v-5" />
      <path d="M12 3v11.5M8.5 11 12 14.5 15.5 11" />
    </Svg>
  );
}

/** Marked billed: an invoice sheet with a round stamp pressed on its corner. */
export function StepBilledIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3.5" y="3.5" width="14" height="17" rx="2.5" />
      <path d="M7 8h6M7 11.5h3" />
      <circle cx="16" cy="16.5" r="4.8" fill="var(--bg, transparent)" />
      <circle cx="16" cy="16.5" r="4.8" {...WASH} />
      <circle cx="16" cy="16.5" r="4.8" />
      <path d="m13.8 16.6 1.6 1.6 2.9-3.2" />
    </Svg>
  );
}
