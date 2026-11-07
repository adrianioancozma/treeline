import type { MouseEvent } from "react";
import { create } from "zustand";

export interface MenuItem {
  label?: string;
  onClick?: () => void;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
  hint?: string;
}

interface UiState {
  menu: { x: number; y: number; items: MenuItem[] } | null;
  /** Interactive-rebase dialog request. `base` is the commit to rebase onto (null = root). */
  rebase: { tabId: string; base: string | null; title: string } | null;
  clone: boolean;
  settings: boolean;
}

export const useUi = create<UiState>(() => ({ menu: null, rebase: null, clone: false, settings: false }));

export function openMenu(e: MouseEvent | { clientX: number; clientY: number; preventDefault?: () => void; stopPropagation?: () => void }, items: MenuItem[]) {
  e.preventDefault?.();
  e.stopPropagation?.();
  useUi.setState({ menu: { x: e.clientX, y: e.clientY, items } });
}

export const closeMenu = () => useUi.setState({ menu: null });
