import { describe, expect, it } from "bun:test";
import { principalAxes, type Vec3 } from "./starfieldScene";

const orthonormal = (axes: Vec3[]) => {
  for (let i = 0; i < 3; i++) {
    expect(Math.hypot(...axes[i])).toBeCloseTo(1, 9);
    for (let j = i + 1; j < 3; j++) expect(axes[i][0] * axes[j][0] + axes[i][1] * axes[j][1] + axes[i][2] * axes[j][2]).toBeCloseTo(0, 9);
  }
};

describe("principalAxes on degenerate clouds stays an orthonormal basis (the view turn stays rigid)", () => {
  it("a straight line", () => {
    const line = Array.from({ length: 7 }, (_, i): Vec3 => [i * 5, i * 5, 0]);
    const axes = principalAxes(line);
    orthonormal(axes); // catches axes[1] == axes[0] from normalised float noise
    expect(Math.abs(axes[0][0])).toBeCloseTo(Math.SQRT1_2, 6);
  });

  it("a flat plane", () => {
    const plane = [0, 1, 2, 3].flatMap((i) => [0, 1, 2].map((j): Vec3 => [i * 10, j * 3, 0]));
    const axes = principalAxes(plane);
    orthonormal(axes);
    expect(Math.abs(axes[2][2])).toBeCloseTo(1, 6); // the empty direction is the plane's normal
  });

  it("all points the same", () => {
    orthonormal(principalAxes([[1, 2, 3], [1, 2, 3], [1, 2, 3]]));
  });
});
