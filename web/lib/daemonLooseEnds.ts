// Daemon client fn for the billing-cycle loose-ends list (spec 021, FR-11).

import { call } from "./daemon";
import type { LooseEnds } from "./time_leaks_contract";

export async function fetchLooseEnds(): Promise<LooseEnds> {
  return call<LooseEnds>("GET", "/loose-ends");
}
