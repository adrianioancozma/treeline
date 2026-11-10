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

/** The smallest the commit detail panel may be squeezed to by the splitter above it. */
export const MIN_DETAIL_H = 220;
const SPLITTER_H = 6;
const MIN_LIST_H = 140;

/**
 * Height range of the History list, given the height of the area it shares with the commit detail panel. By default
 * the list takes 70% and follows the window; a dragged height is kept. There is no fixed maximum: only enough room is
 * left for the detail panel. `windowH` gives the old default (60% of the window) until the area has been measured.
 */
export function historyListHeights(areaH: number, windowH = 0): { auto: number; min: number; max: number } {
  if (areaH <= 0) return { auto: Math.round(windowH * 0.6), min: MIN_LIST_H, max: Infinity };
  return {
    auto: Math.round(areaH * 0.7),
    min: MIN_LIST_H,
    max: Math.max(MIN_LIST_H, areaH - SPLITTER_H - MIN_DETAIL_H),
  };
}
