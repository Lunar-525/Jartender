import { describe, expect, it } from "vitest";
import { carouselSlot, curvePose, insertionFrames } from "./instanceMotion";

describe("reference trajectory", () => {
  it("leaves the current-instance gap and bends adjacent cards inward", () => {
    const above = curvePose(carouselSlot(5, 6));
    const below = curvePose(carouselSlot(6, 6));
    const far = curvePose(carouselSlot(3, 6));
    expect(above.y).toBeCloseTo(-108.0215, 3);
    expect(below.y).toBeCloseTo(108.0215, 3);
    expect(above.x).toBeCloseTo(below.x, 5);
    expect(above.x).toBeGreaterThan(-310);
    expect(far.x).toBeLessThan(-329);
    expect(curvePose(0).x).toBe(0);
  });
  it("fades offscreen cards and ends every insertion exactly in the dock", () => {
    expect(curvePose(-8).opacity).toBe(0);
    expect(curvePose(4).opacity).toBe(0);
    for (const slot of [-5, -1, 1, 2]) {
      const frames = insertionFrames(slot);
      expect(frames.x.at(-1)).toBe(0);
      expect(frames.y.at(-1)).toBe(0);
      expect(frames.scale.at(-1)).toBe(1);
      expect(frames.x[1]).not.toBeCloseTo(frames.x[0] * 0.7, 1);
    }
  });
});
