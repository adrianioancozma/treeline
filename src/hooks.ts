import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";

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
 * A draggable divider. Returns the current size and a mouse-down handler for the handle.
 * `axis` "x" measures from the container's left edge, "y" from its top edge.
 */
export function useSplit(key: string, initial: number, min: number, max: number, axis: "x" | "y") {
  const [size, setSize] = useState(() => readSize(`split:${key}`, initial, min, max));
  const sizeRef = useRef(size);
  sizeRef.current = size;

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const start = axis === "x" ? e.clientX : e.clientY;
      const startSize = sizeRef.current;
      const move = (ev: MouseEvent) => {
        const delta = (axis === "x" ? ev.clientX : ev.clientY) - start;
        setSize(Math.min(max, Math.max(min, startSize + delta)));
      };
      const up = () => {
        window.removeEventListener("mousemove", move);
        window.removeEventListener("mouseup", up);
        document.body.classList.remove("resizing");
        try {
          localStorage.setItem(`split:${key}`, String(sizeRef.current));
        } catch {
          /* storage unavailable: the size just won't persist */
        }
      };
      document.body.classList.add("resizing");
      window.addEventListener("mousemove", move);
      window.addEventListener("mouseup", up);
    },
    [axis, key, min, max],
  );
  return [size, onMouseDown] as const;
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
