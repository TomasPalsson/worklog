import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { MapPoint, Topic, TopicsReport } from "@/lib/topics_contract";
import StarField from "./StarField";

afterEach(cleanup);

const topic = (id: number, name: string): Topic => ({
  id,
  name,
  description: null,
  named_by: "keywords",
  keywords: [],
  seconds: 3600,
  blocks: 1,
  days_active: 1,
  first_day: "2026-10-01",
  last_day: "2026-10-01",
  top_tickets: [],
});

const point = (block_id: number, topic_id: number): MapPoint => ({
  block_id,
  x: 0.5,
  y: 0.5,
  sx: 0.4 + block_id / 20,
  sy: 0.5,
  sz: 0.5,
  topic_id,
  day: "2026-10-01",
  seconds: 3600,
  label: `block ${block_id}`,
  ticket: null,
  novelty: 0.5,
  oddball: false,
});

const report: TopicsReport = {
  status: "ready",
  from: "2026-10-01",
  to: "2026-10-07",
  naming: "off",
  topics: [topic(0, "Billing export"), topic(1, "Review UI")],
  points: [point(1, 0), point(2, 0), point(3, 1)],
  bin: "day",
  river: [],
  days: [],
  day_similarity: [],
  first_times: [],
  oddballs: [],
  oddball_median: 0,
  embedded_blocks: 3,
  skipped_blocks: 0,
};

const HINT = "Click the field, then W A S D to fly · drag to look · Shift faster · R reset · Esc leave";

describe("StarField", () => {
  it("shows the fallback text when there is no canvas context", () => {
    const { getByText } = render(<StarField report={report} />);
    expect(getByText("Star field needs canvas support.")).toBeTruthy();
  });

  it("labels the canvas with block and topic counts", () => {
    const { getByRole } = render(<StarField report={report} />);
    expect(getByRole("img").getAttribute("aria-label")).toBe("3-D star field: 3 blocks in 2 topics");
  });

  it("renders one legend chip per topic with its name", () => {
    const { getByText, container } = render(<StarField report={report} />);
    expect(getByText("Billing export")).toBeTruthy();
    expect(getByText("Review UI")).toBeTruthy();
    expect(container.querySelectorAll(".sx-star-chip").length).toBe(2);
  });

  it("has a focusable wrapper described by the hint", () => {
    const { container, getByText } = render(<StarField report={report} />);
    const wrap = container.querySelector(".sx-star") as HTMLElement;
    expect(wrap.tabIndex).toBe(0);
    const hint = getByText(HINT);
    expect(hint.id).not.toBe("");
    expect(wrap.getAttribute("aria-describedby")).toBe(hint.id);
  });

  it("keydown w on the wrapper is prevented so the page does not scroll", () => {
    const { container } = render(<StarField report={report} />);
    const wrap = container.querySelector(".sx-star") as HTMLElement;
    expect(fireEvent.keyDown(wrap, { key: "w" })).toBe(false);
    expect(fireEvent.keyDown(wrap, { key: "ArrowDown" })).toBe(false);
  });

  it("leaves keys it does not handle alone (Tab must still move focus)", () => {
    const { container } = render(<StarField report={report} />);
    const wrap = container.querySelector(".sx-star") as HTMLElement;
    expect(fireEvent.keyDown(wrap, { key: "Tab" })).toBe(true);
  });

  it("keydown on document.body is not prevented", () => {
    render(<StarField report={report} />);
    expect(fireEvent.keyDown(document.body, { key: "w" })).toBe(true);
    expect(fireEvent.keyDown(document.body, { key: "ArrowDown" })).toBe(true);
  });

  it("Escape blurs the wrapper", () => {
    const { container } = render(<StarField report={report} />);
    const wrap = container.querySelector(".sx-star") as HTMLElement;
    wrap.focus();
    expect(document.activeElement).toBe(wrap);
    fireEvent.keyDown(wrap, { key: "Escape" });
    expect(document.activeElement).not.toBe(wrap);
  });
});
