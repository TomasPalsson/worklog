import { describe, expect, it } from "bun:test";
import { rowChip, statusKind, statusMeta } from "./mirresStatus";

const b = (
  class_: "billable" | "included" | "not_billable",
  project_type: string | null = null,
  warning: string | null = null,
) => ({ class: class_, project_type, warning });

describe("statusKind", () => {
  it("flags missing-contract warnings over the class", () => {
    expect(statusKind(b("billable", null, "Samning vantar í Mirres"))).toBe("missing");
    expect(statusKind(b("included", null, "Tímafjölda vantar á samning"))).toBe("missing");
    expect(statusKind(b("billable", null, "Fleiri en ein samningur"))).toBe("missing");
    expect(statusKind(b("billable", null, "Óþekkt verkefni"))).toBe("missing");
    expect(statusKind(b("billable", null, "Ekki virkt Mirres-verkefni"))).toBe("missing");
  });
  it("other warnings keep the class", () => {
    expect(statusKind(b("included", null, "Innifaldir tímar uppurnir"))).toBe("included");
  });
  it("maps billable, included and not-billable types", () => {
    expect(statusKind(b("billable"))).toBe("billable");
    expect(statusKind(b("included"))).toBe("included");
    expect(statusKind(b("not_billable", "Fast verð"))).toBe("fixed");
    expect(statusKind(b("not_billable", "Innifalið í vöruáskrift"))).toBe("fixed");
    expect(statusKind(b("not_billable", "Innri (Apró)"))).toBe("internal");
    expect(statusKind(b("not_billable", "Annað"))).toBe("not_billable");
  });
});

describe("statusMeta", () => {
  it("labels subscriptions and tones kinds", () => {
    expect(statusMeta(b("not_billable", "Innifalið í vöruáskrift")).label).toBe("subscription");
    expect(statusMeta(b("not_billable", "Fast verð")).label).toBe("fixed price");
    expect(statusMeta(b("billable")).tone).toBe("sage");
    expect(statusMeta(b("billable", null, "Samning vantar")).tone).toBe("amber");
  });
});

describe("rowChip", () => {
  it("shows contract trouble and used-up or low hours on the row itself", () => {
    expect(rowChip(b("billable", null, "Samning vantar í Mirres"))).toBe("contract missing");
    expect(rowChip(b("not_billable", "Innifalið í vöruáskrift", "Innifaldir tímar uppurnir"))).toBe("hours used up");
    expect(rowChip(b("included", null, "Innifaldir tímar að klárast (1 klst eftir)"))).toBe("hours running low");
  });
  it("stays quiet when nothing needs attention", () => {
    expect(rowChip(b("billable"))).toBeNull();
    expect(rowChip(b("not_billable", "Innifalið í vöruáskrift"))).toBeNull();
  });
});
