import { describe, expect, it } from "bun:test";
import { billingDetailsText } from "./mirresDetails";
import type { MirresDetails } from "./tempo_line_contract";

const base: MirresDetails = {
  customer_name: null,
  owner: null,
  responsible: null,
  team_lead: null,
  period: null,
  allowance_hours: null,
  used_hours: null,
  remaining_hours: null,
  usage_status: null,
  due_date: null,
  contract_url: null,
};

describe("billingDetailsText", () => {
  it("owner only", () => {
    expect(billingDetailsText({ ...base, owner: { name: "Anna", email: "a@x.is" } })).toEqual([
      { kind: "person", label: "Owner", name: "Anna", email: "a@x.is" },
    ]);
  });

  it("drops the customer lead when it is the owner", () => {
    const p = { name: "Anna", email: null };
    const parts = billingDetailsText({ ...base, owner: p, responsible: p });
    expect(parts).toHaveLength(1);
    const other = billingDetailsText({ ...base, owner: p, responsible: { name: "Jón", email: null } });
    expect(other[1]).toMatchObject({ label: "Customer lead", name: "Jón" });
  });

  it("hours with remaining, per period", () => {
    const t = (period: string) =>
      billingDetailsText({ ...base, period, allowance_hours: 10, remaining_hours: 1.5 })[0];
    expect(t("MONTHLY")).toEqual({ kind: "text", text: "1,5 of 10 h left this month" });
    expect(t("YEARLY")).toEqual({ kind: "text", text: "1,5 of 10 h left this year" });
    expect(t("ONE_OFF")).toEqual({ kind: "text", text: "1,5 of 10 h left in total" });
  });

  it("hours without remaining, per period", () => {
    const t = (period: string) => billingDetailsText({ ...base, period, allowance_hours: 10 })[0];
    expect(t("MONTHLY")).toEqual({ kind: "text", text: "10 h included this month" });
    expect(t("YEARLY")).toEqual({ kind: "text", text: "10 h included this year" });
    expect(t("ONE_OFF")).toEqual({ kind: "text", text: "10 h included in total" });
  });

  it("due date and contract link", () => {
    const parts = billingDetailsText({ ...base, due_date: "2026-12-31", contract_url: "https://m/c" });
    expect(parts).toEqual([
      { kind: "text", text: "Due 2026-12-31" },
      { kind: "link", text: "Contract", href: "https://m/c" },
    ]);
  });

  it("empty", () => {
    expect(billingDetailsText(base)).toEqual([]);
    expect(billingDetailsText(null)).toEqual([]);
  });
});
