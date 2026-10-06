import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { MirresLine } from "./MirresLine";
import type { LineBilling } from "@/lib/tempo_line_contract";

afterEach(cleanup);

const billing = (o: Partial<LineBilling> = {}): LineBilling => ({
  account_key: "ACME-1",
  project: "Coripharma · Vefur",
  project_type: "Tímavinna",
  class: "billable",
  warning: null,
  customer: "Coripharma",
  details: null,
  ...o,
});

describe("MirresLine", () => {
  it("shows customer and project without the customer prefix", () => {
    render(<MirresLine billing={billing()} issue="PROJ-1" />);
    expect(screen.getAllByText("Coripharma").length).toBeGreaterThan(0);
    expect(screen.getByText("· Vefur")).toBeTruthy();
  });

  it("has a labelled info button wired to a popover", () => {
    render(<MirresLine billing={billing()} issue="PROJ-1" />);
    const btn = screen.getByRole("button", { name: "Mirres details for PROJ-1" });
    expect(btn.getAttribute("popovertarget")).toBeTruthy();
    expect(document.getElementById(btn.getAttribute("popovertarget")!)?.getAttribute("popover")).toBe("auto");
  });

  it("names a missing contract in text, not colour alone", () => {
    render(<MirresLine billing={billing({ warning: "Samning vantar í Mirres" })} issue="PROJ-1" />);
    expect(screen.getAllByText("contract missing").length).toBeGreaterThan(0);
  });
});
