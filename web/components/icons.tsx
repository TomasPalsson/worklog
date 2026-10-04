// worklog's own icon set. One hand: 24px grid, 1.6 stroke, round joins,
// a soft currentColor wash (opacity .16) for the "filled" part of each
// object. Decorative: every icon is aria-hidden; the control that holds
// it carries the label.

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

/** worklog mark: a "w" drawn as a running total over a baseline. */
export function LogoMark(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="2.5" y="2.5" width="19" height="19" rx="5.5" {...WASH} />
      <path d="M6.5 9l2.4 7.2L12 10.8l3.1 5.4L17.5 9" strokeWidth={1.9} />
    </Svg>
  );
}

/** Day: a clock face with the worked slice of the day washed in. */
export function DayIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M12 12V3.5A8.5 8.5 0 0 1 20.5 12z" {...WASH} opacity={0.42} />
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7v5l3.2 2" />
    </Svg>
  );
}

/** Week: a calendar sheet with this week's row picked out. */
export function WeekIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3.5" y="4.5" width="17" height="16" rx="2.5" />
      <path d="M3.5 9h17M8 3v3M16 3v3" />
      <rect x="5.5" y="11.6" width="13" height="3.6" rx="1.4" {...WASH} opacity={0.5} />
      <path d="M7.5 18h.01M12 18h.01M16.5 18h.01" strokeWidth={2.4} opacity={0.6} />
    </Svg>
  );
}

/** Tasks: a ticket card checked off, with the next one behind it. */
export function TasksIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M8 4.8V4.5A1.5 1.5 0 0 1 9.5 3h9A2.5 2.5 0 0 1 21 5.5v10a1.5 1.5 0 0 1-1.5 1.5h-.3" opacity={0.55} />
      <rect x="3" y="6.5" width="14.5" height="14.5" rx="2.5" {...WASH} />
      <rect x="3" y="6.5" width="14.5" height="14.5" rx="2.5" />
      <path d="M6.6 13.6l2.2 2.2 4.4-4.8" strokeWidth={1.8} />
    </Svg>
  );
}

/** Logged: a timesheet whose rows are fill meters — full, most, short. */
export function LoggedIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3.5" y="2.5" width="17" height="19" rx="3" />
      <path d="M7.5 8h9M7.5 12h9M7.5 16h9" opacity={0.22} strokeWidth={2.6} />
      <path d="M7.5 8h9M7.5 12h6M7.5 16h2.5" strokeWidth={2.6} />
    </Svg>
  );
}

/** Settings: three sliders at different settings. */
export function SettingsIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M4 6.5h16M4 12h16M4 17.5h16" opacity={0.55} />
      <circle cx="9" cy="6.5" r="2.3" fill="var(--bg, transparent)" />
      <circle cx="15.5" cy="12" r="2.3" fill="var(--bg, transparent)" />
      <circle cx="7" cy="17.5" r="2.3" fill="var(--bg, transparent)" />
    </Svg>
  );
}

/** Billing: a till receipt with a torn edge. */
export function BillingIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M5.5 3h13v18l-2.2-1.4-2.1 1.4-2.2-1.4-2.2 1.4-2.1-1.4-2.2 1.4z" {...WASH} />
      <path d="M5.5 3h13v18l-2.2-1.4-2.1 1.4-2.2-1.4-2.2 1.4-2.1-1.4-2.2 1.4z" />
      <path d="M9 8h6M9 11.5h6M9 15h3.4" />
    </Svg>
  );
}

/** Month view: a calendar sheet with its days dotted in. */
export function MonthViewIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3.5" y="4.5" width="17" height="16" rx="2.5" />
      <path d="M3.5 9h17M8 3v3M16 3v3" />
      <path d="M7.5 12.5h.01M12 12.5h.01M16.5 12.5h.01M7.5 16.5h.01M12 16.5h.01" strokeWidth={2.4} />
      <rect x="14.6" y="14.6" width="3.8" height="3.8" rx="1" {...WASH} opacity={0.45} />
    </Svg>
  );
}

/** Week view: workday columns filled to how much was logged. */
export function WeekViewIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M3.5 20.5h17" opacity={0.55} />
      <path d="M6 18V5M10 18V5M14 18V5M18 18V5" opacity={0.22} strokeWidth={2.6} />
      <path d="M6 18V5M10 18V8.5M14 18V11M18 18V6.5" strokeWidth={2.6} />
    </Svg>
  );
}

/** Day view: one column of logged entries. */
export function DayViewIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="6" y="3.5" width="12" height="17" rx="2.5" />
      <path d="M9 8.5h6M9 12h6M9 15.5h3.5" />
    </Svg>
  );
}

export function PrevIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M14.5 6l-6 6 6 6" strokeWidth={1.9} />
    </Svg>
  );
}

export function NextIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M9.5 6l6 6-6 6" strokeWidth={1.9} />
    </Svg>
  );
}

/** Refresh from Tempo: two arcs chasing each other. */
export function RefreshIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M19.5 12a7.5 7.5 0 0 1-12.8 5.3" />
      <path d="M4.5 12a7.5 7.5 0 0 1 12.8-5.3" />
      <path d="M17.6 3.2v3.6H14M6.4 20.8v-3.6H10" />
    </Svg>
  );
}

/** Theme: light. */
export function SunIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="4" {...WASH} />
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2.8v2M12 19.2v2M2.8 12h2M19.2 12h2M5.5 5.5l1.4 1.4M17.1 17.1l1.4 1.4M5.5 18.5l1.4-1.4M17.1 6.9l1.4-1.4" />
    </Svg>
  );
}

/** Theme: dark. */
export function MoonIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M19.5 14.6A8 8 0 0 1 9.4 4.5a8 8 0 1 0 10.1 10.1z" {...WASH} />
      <path d="M19.5 14.6A8 8 0 0 1 9.4 4.5a8 8 0 1 0 10.1 10.1z" />
    </Svg>
  );
}

/** Theme: follow the system — a half-lit disc. */
export function SystemThemeIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="8" />
      <path d="M12 4a8 8 0 0 1 0 16z" fill="currentColor" stroke="none" opacity={0.55} />
    </Svg>
  );
}
