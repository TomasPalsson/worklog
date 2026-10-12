// Daemon client fn for the billing-cycle loose-ends list (spec 021, FR-11).

import { call } from "./daemon";
import type { UnbilledDays } from "./time_leaks_contract";

export async function fetchLooseEnds(): Promise<UnbilledDays> {
  return call<UnbilledDays>("GET", "/loose-ends");
}
