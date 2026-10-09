import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { RoutingPipeline, ageMs, formatAge, freshness } from "./RoutingPipeline";

const NOW = Date.parse("2026-10-09T12:00:00Z");
const ago = (ms: number) => new Date(NOW - ms).toISOString();

afterEach(cleanup);

describe("routing pipeline maths", () => {
  it("buckets freshness", () => {
    expect(freshness(119 * 60_000)).toBe("ok");
    expect(freshness(120 * 60_000)).toBe("stale");
    expect(freshness(24 * 3_600_000)).toBe("bad");
    expect(freshness(null)).toBe("never");
  });
  it("formats ages and clamps skew / bad input", () => {
    expect(formatAge(2 * 60_000)).toBe("2 min");
    expect(formatAge(3 * 3_600_000)).toBe("3 h");
    expect(formatAge(4 * 86_400_000)).toBe("4 d");
    expect(formatAge(null)).toBe("never");
    expect(ageMs(ago(-5000), NOW)).toBe(0);
    expect(ageMs("nope", NOW)).toBeNull();
  });
});

describe("<RoutingPipeline />", () => {
  it("labels all three stages and breaks the connector after a stale one", () => {
    const { container, getByRole } = render(
      <RoutingPipeline
        now={NOW}
        status={{ last_heartbeat: ago(120_000), last_slack: ago(3 * 3_600_000), classifier_reachable: true }}
      />,
    );
    const label = getByRole("img").getAttribute("aria-label")!;
    expect(label).toContain("Browser 2 min (fresh)");
    expect(label).toContain("Slack 3 h (stale)");
    expect(label).toContain("Model up (reachable)");
    const conns = container.querySelectorAll(".art-settings-conn");
    expect(conns[0].hasAttribute("data-broken")).toBe(false);
    expect(conns[1].hasAttribute("data-broken")).toBe(true);
  });
  it("null timestamps read never; null status is all down", () => {
    const { getByRole, container } = render(<RoutingPipeline now={NOW} status={null} />);
    expect(getByRole("img").getAttribute("aria-label")).toContain("Browser never (never seen)");
    expect(container.querySelectorAll('[data-health="never"]').length).toBe(2);
    expect(container.querySelectorAll('[data-health="bad"]').length).toBe(1);
  });
});
