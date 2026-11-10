// Sizes for panes and columns that have an automatic width until the user drags them.

/** The user's width when there is one, otherwise the automatic width; always inside [min, max]. */
export function autoSize(stored: number | null, auto: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, stored ?? auto));
}

/** Width range of the history Graph column: automatic up to 12 lanes, at least one lane, no maximum (P26). */
export function graphWidths(lanes: number, laneW: number): { auto: number; min: number; max: number } {
  const w = (n: number) => n * laneW + 8;
  const auto = w(Math.min(12, lanes));
  return { auto, min: w(1), max: Infinity };
}
