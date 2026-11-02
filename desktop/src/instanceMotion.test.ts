import { describe, expect, it } from "vitest";
import { carouselSlot, curvePose } from "./instanceMotion";

describe("reference trajectory", () => {
  it("places the preview in the current-instance dock and bends neighbours inward", () => {
    const above = curvePose(carouselSlot(5, 6));
    const below = curvePose(carouselSlot(7, 6));
    const far = curvePose(carouselSlot(3, 6));
    expect(above.y).toBe(-110);
    expect(below.y).toBe(110);
    expect(carouselSlot(6, 6)).toBe(0);
    expect(above.x).toBeCloseTo(below.x, 5);
    expect(above.x).toBeGreaterThan(-310);
    expect(far.x).toBeLessThan(-329);
    expect(curvePose(0).x).toBe(0);
  });
  it("fades offscreen cards and ends every insertion exactly in the dock", () => {
    expect(curvePose(-8).opacity).toBe(0);
    expect(curvePose(4).opacity).toBe(0);
    for (const slot of [-5, -1, 1, 2]) {
      const halfway = curvePose(slot * 0.5);
      const end = curvePose(slot * (1 - 1));
      expect(end.x).toBe(0);
      expect(end.y).toBe(0);
      expect(end.scale).toBe(1);
      expect(halfway.x).not.toBeCloseTo(curvePose(slot).x * 0.5, 1);
    }
  });
});
