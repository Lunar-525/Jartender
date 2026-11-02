export const INSTANCE_SURFACE = { width: 1850, height: 1100, gutter: 350 };
export const INSTANCE_SPRING = { stiffness: 380, damping: 34, mass: 1 };
export const INSERT_SECONDS = 0.36;
export const FADE_SECONDS = 0.18;

/** The outer column bends inward as a card approaches the current-instance dock. */
export function curvePose(slot: number) {
  const distance = Math.abs(slot);
  const edgeFade = Math.max(0, Math.min(1, (slot + 7.1) / 1.2, (2.9 - slot) / 0.9));
  return {
    x: -330 * (1 - Math.exp(-2.6 * slot * slot)),
    y: slot * 110,
    scale: 1 - Math.min(distance, 6) * 0.012,
    opacity: edgeFade * (0.94 - Math.min(distance, 6) * 0.095),
  };
}

/** Integer anchors leave a slot for the current instance between two neighbours. */
export function carouselSlot(index: number, anchor: number) {
  const distance = index - anchor + 0.5;
  return distance + 0.5 * Math.tanh(4 * distance);
}

export function clampAnchor(anchor: number, length: number) {
  return Math.max(0, Math.min(length, Math.round(anchor)));
}

export function insertionFrames(slot: number) {
  const frames = [slot, slot * 0.7, slot * 0.35, 0].map(curvePose);
  return {
    x: frames.map(frame => frame.x),
    y: frames.map(frame => frame.y),
    scale: frames.map(frame => frame.scale),
    opacity: [0.95, 1, 1, 1],
  };
}
