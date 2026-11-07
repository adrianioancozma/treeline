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

const SECTIONS_KEY = "sidebar-sections";

/** Which sidebar sections the user has opened or closed, per tab (tab id → section key → open). */
type TabSections = Record<string, Record<string, boolean>>;

function loadSections(): TabSections {
  try {
    const v = JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? "{}");
    return v && typeof v === "object" ? v : {};
  } catch {
    return {}; // storage unavailable or corrupt: fall back to the defaults
  }
}

/** A request to show one file's history or blame. */
export interface FileHistoryRequest {
  /** Repository folder. */
  repo: string;
  /** File path relative to the repository. */
  file: string;
  mode: "history" | "blame";
  /** Blame the file as of this commit instead of the working copy. */
  rev?: string;
}

interface UiState {
  fileHistory: FileHistoryRequest | null;
  /** Sidebar sections the user toggled by hand, per tab; absent = use the section's default. */
  sections: TabSections;
  menu: { x: number; y: number; items: MenuItem[] } | null;
  /** Interactive-rebase dialog request. `base` is the commit to rebase onto (null = root). */
  rebase: { tabId: string; base: string | null; title: string } | null;
  clone: boolean;
  settings: boolean;
}

export const useUi = create<UiState>(() => ({ fileHistory: null, sections: loadSections(), menu: null, rebase: null, clone: false, settings: false }));

export function openMenu(e: MouseEvent | { clientX: number; clientY: number; preventDefault?: () => void; stopPropagation?: () => void }, items: MenuItem[]) {
  e.preventDefault?.();
  e.stopPropagation?.();
  useUi.setState({ menu: { x: e.clientX, y: e.clientY, items } });
}

export const closeMenu = () => useUi.setState({ menu: null });

function saveSections(sections: TabSections) {
  useUi.setState({ sections });
  try {
    localStorage.setItem(SECTIONS_KEY, JSON.stringify(sections));
  } catch {
    /* not remembered across restarts, still kept while the app runs */
  }
}

/** Remembers that the user opened or closed a sidebar section in this tab. */
export function setSectionOpen(tabId: string, key: string, open: boolean) {
  const all = useUi.getState().sections;
  saveSections({ ...all, [tabId]: { ...all[tabId], [key]: open } });
}

/** Drops a closed tab's remembered sections. */
export function forgetSections(tabId: string) {
  const { [tabId]: _gone, ...rest } = useUi.getState().sections;
  if (_gone) saveSections(rest);
}
