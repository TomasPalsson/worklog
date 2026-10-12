import { describe, expect, it } from "bun:test";
import type { MapPoint, TopicsReport } from "@/lib/topics_contract";
import { type GalaxyDot, type GalaxyLabel, galaxyLayout } from "./galaxyLayout";

const point = (block_id: number, topic_id: number, x: number, y: number): MapPoint => ({
  block_id,
  x,
  y,
  sx: 0.5,
  sy: 0.5,
  sz: 0.5,
  topic_id,
  day: "2026-10-01",
  seconds: 3600,
  label: "l",
  ticket: null,
  novelty: 0.5,
  oddball: false,
});
const topic = (id: number, name: string) => ({ id, name, seconds: 3600 * (3 - id) });
const report = (points: MapPoint[]) =>
  ({ topics: [topic(0, "Alpha topic"), topic(1, "Beta topic"), topic(2, "Gamma topic")], points }) as unknown as TopicsReport;

const covers = (l: GalaxyLabel, d: GalaxyDot) => {
  const nx = Math.min(l.x + l.w / 2, Math.max(l.x - l.w / 2, d.cx));
  const ny = Math.min(l.y + l.h / 2, Math.max(l.y - l.h / 2, d.cy));
  return (nx - d.cx) ** 2 + (ny - d.cy) ** 2 < d.r ** 2;
};

describe("map labels never cover their own island", () => {
  it("a topic at the top edge with another island right below it keeps its label off its own dots", () => {
    // the adversary's canary: A at y≈0 (no room above), B just under it (below is blocked)
    const pts = [
      point(1, 0, 0.48, 0),
      point(2, 0, 0.5, 0.01),
      point(3, 0, 0.52, 0),
      point(4, 1, 0.47, 0.09),
      point(5, 1, 0.5, 0.1),
      point(6, 1, 0.53, 0.09),
      point(7, 2, 0.9, 1),
      point(8, 2, 0.92, 0.98),
    ];
    const { labels, dots } = galaxyLayout(report(pts));
    for (const l of labels) {
      const own = dots.filter((d) => d.point.topic_id === l.topic.id);
      expect(own.filter((d) => covers(l, d))).toHaveLength(0);
    }
  });
});
