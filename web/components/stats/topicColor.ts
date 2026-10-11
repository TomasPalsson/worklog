export const topicHue = (id: number, k: number) => 25 + id * (360 / Math.max(1, k));

export const topicColor = (id: number, k: number) =>
  `oklch(calc(var(--project-l) + ${id % 2 === 0 ? 0 : -0.08}) var(--project-c) ${topicHue(id, k)})`;
