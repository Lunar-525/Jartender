export const INSTANCE_SURFACE = { width: 1850, height: 1100, gutter: 350 };
export const INSTANCE_SPRING = { stiffness: 380, damping: 34, mass: 1 };
export const ALIGN_SECONDS = 0.36;
export const FADE_SECONDS = 0.18;

/** The outer column bends inward as a card approaches the current-instance dock. */
export function curvePose(slot: number) {
  const distance = Math.abs(slot);
  const edgeFade = Math.max(
    0,
    Math.min(1, (slot + 7.1) / 1.2, (2.9 - slot) / 0.9),
  );
  return {
    x: slot === 0 ? 0 : -330 * (1 - Math.exp(-2.6 * slot * slot)),
    y: slot === 0 ? 0 : slot * 110,
    scale: 1 - Math.min(distance, 6) * 0.012,
    opacity: edgeFade * (0.94 - Math.min(distance, 6) * 0.095),
  };
}

/** The previewed instance occupies the dock at each integer detent. */
export function carouselSlot(index: number, anchor: number) {
  return index - anchor;
}

export function clampAnchor(anchor: number, lastIndex: number) {
  return Math.max(0, Math.min(lastIndex, Math.round(anchor)));
}
