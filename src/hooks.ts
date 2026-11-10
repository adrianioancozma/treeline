import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { autoSize } from "./lib/paneSize";

/** The installed app version, or null until it is known (or if it cannot be read). */
export function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    api.appVersion().then(setVersion, () => undefined);
  }, []);
  return version;
}

/** Reads a saved pane size, clamped to the allowed range so a stale or corrupt value cannot break the layout. */
function readSize(key: string, fallback: number, min: number, max: number): number {
  try {
    const v = Number(localStorage.getItem(key));
    return Number.isFinite(v) && v > 0 ? Math.min(max, Math.max(min, v)) : fallback;
  } catch {
    return fallback;
  }
}

/**
 * Follows the pointer for a divider drag until the button is released. A release outside the web view (e.g. over the
 * window's title bar) may never arrive as mouseup, so a move with no button pressed, or the window losing focus, also
 * ends the drag; otherwise the divider would keep following the pointer.
 */
function trackDrag(onMove: (ev: MouseEvent) => void, onEnd: () => void) {
  const move = (ev: MouseEvent) => {
    if (ev.buttons === 0) return end();
    onMove(ev);
  };
  const end = () => {
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", end);
    window.removeEventListener("blur", end);
    document.body.classList.remove("resizing");
    onEnd();
  };
  document.body.classList.add("resizing");
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", end);
  window.addEventListener("blur", end);
}

/**
 * Like useSplit for a column whose natural width depends on its content (`auto`): it follows `auto` until the user
 * drags the handle, remembers the dragged width, and goes back to automatic on reset (double-click).
 */
export function useAutoSplit(key: string, auto: number, min: number, max: number) {
  const [stored, setStored] = useState<number | null>(() => {
    try {
      const v = Number(localStorage.getItem(`split:${key}`));
      return Number.isFinite(v) && v > 0 ? v : null;
    } catch {
      return null;
    }
  });
  const size = autoSize(stored, auto, min, max);
  const sizeRef = useRef(size);
  sizeRef.current = size;

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const start = e.clientX;
      const startSize = sizeRef.current;
      // The last value the pointer produced: on a quick release the render for the last move may not have happened yet.
      let latest = startSize;
      trackDrag(
        (ev) => {
          latest = Math.min(max, Math.max(min, startSize + ev.clientX - start));
          setStored(latest);
        },
        () => {
          try {
            localStorage.setItem(`split:${key}`, String(latest));
          } catch {
            /* storage unavailable: the size just won't persist */
          }
        },
      );
    },
    [key, min, max],
  );
  const reset = useCallback(() => {
    setStored(null);
    try {
      localStorage.removeItem(`split:${key}`);
    } catch {
      /* storage unavailable */
    }
  }, [key]);
  return [size, onMouseDown, reset] as const;
}

/**
 * A draggable divider. Returns the current size and a mouse-down handler for the handle.
 * `axis` "x" measures from the container's left edge, "y" from its top edge.
 * `invert` is for a handle on the near edge of a pane that grows when dragged the other way (e.g. a right-hand column).
 */
export function useSplit(key: string, initial: number, min: number, max: number, axis: "x" | "y", invert = false) {
  const [stored, setSize] = useState(() => readSize(`split:${key}`, initial, min, max));
  // `max` can shrink (a smaller window), so the remembered size is always clamped to the current range:
  // a stale value must never push the pane after it off screen.
  const size = Math.min(max, Math.max(min, stored));
  const sizeRef = useRef(size);
  sizeRef.current = size;

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const start = axis === "x" ? e.clientX : e.clientY;
      const startSize = sizeRef.current;
      let latest = startSize;
      trackDrag(
        (ev) => {
          const delta = (axis === "x" ? ev.clientX : ev.clientY) - start;
          latest = Math.min(max, Math.max(min, startSize + (invert ? -delta : delta)));
          setSize(latest);
        },
        () => {
          try {
            localStorage.setItem(`split:${key}`, String(latest));
          } catch {
            /* storage unavailable: the size just won't persist */
          }
        },
      );
    },
    [axis, key, min, max, invert],
  );
  const reset = useCallback(() => {
    setSize(initial);
    try {
      localStorage.removeItem(`split:${key}`);
    } catch {
      /* storage unavailable */
    }
  }, [initial, key]);
  return [size, onMouseDown, reset] as const;
}

/** Debounces a fast-changing value (e.g. a search box). */
export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

/** The live content height of an element, kept up to date as the window or panel is resized. */
export function useElementHeight(ref: React.RefObject<HTMLElement | null>): number {
  const [h, setH] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setH(el.clientHeight);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return h;
}

/**
 * A vertical divider for a box that normally sizes itself to its content. `size` is null until the
 * user drags, then the remembered height (never more than `max`). `start` takes the box's current
 * height so the first drag begins from what is on screen. `reset` goes back to automatic sizing.
 */
export function useOptionalSplit(key: string, min: number, max: number) {
  const [size, setSize] = useState<number | null>(() => {
    try {
      const v = Number(localStorage.getItem(`split:${key}`));
      return Number.isFinite(v) && v > 0 ? v : null;
    } catch {
      return null;
    }
  });
  const sizeRef = useRef(size);
  sizeRef.current = size;
  const shown = size === null ? null : Math.min(max, Math.max(min, size));

  const start = useCallback(
    (e: React.MouseEvent, current: number) => {
      e.preventDefault();
      const y0 = e.clientY;
      const base = Math.min(max, Math.max(min, sizeRef.current ?? current));
      let latest = base;
      trackDrag(
        (ev) => {
          latest = Math.min(max, Math.max(min, base + ev.clientY - y0));
          setSize(latest);
        },
        () => {
          try {
            localStorage.setItem(`split:${key}`, String(latest));
          } catch {
            /* storage unavailable: the size just won't persist */
          }
        },
      );
    },
    [key, min, max],
  );

  const reset = useCallback(() => {
    setSize(null);
    try {
      localStorage.removeItem(`split:${key}`);
    } catch {
      /* ignore */
    }
  }, [key]);

  return [shown, start, reset] as const;
}
