const LIGHTNESS = [0, -0.08, 0.06];

export const topicHue = (id: number, _k: number) => (25 + id * 137.508) % 360;

export const topicColor = (id: number, k: number) =>
  `oklch(calc(var(--project-l) + ${LIGHTNESS[id % 3]}) var(--project-c) ${topicHue(id, k)})`;
