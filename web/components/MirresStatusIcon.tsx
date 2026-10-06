import {
  BillableIcon,
  ContractMissingIcon,
  FixedPriceIcon,
  IncludedIcon,
  InternalIcon,
} from "./mirres-art";
import { MirresIcon } from "./icons";
import { STATUS_META } from "@/lib/mirresStatus";
import type { StatusKind } from "@/lib/mirresStatus";

const ICONS = {
  billable: BillableIcon,
  included: IncludedIcon,
  fixed: FixedPriceIcon,
  internal: InternalIcon,
  missing: ContractMissingIcon,
  not_billable: MirresIcon,
} as const;

/** Decorative status mark; the tone sets its colour via `data-tone`. */
export function MirresStatusIcon({ kind, size = 14 }: { kind: StatusKind; size?: number }) {
  const Icon = ICONS[kind];
  return (
    <span className="mirres-status-icon" data-tone={STATUS_META[kind].tone}>
      <Icon size={size} />
    </span>
  );
}
