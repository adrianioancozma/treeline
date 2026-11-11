const PATHS = {
  fetch: "M12 3v12m0 0l-4-4m4 4l4-4M4 19h16",
  pull: "M12 4v12m0 0l-4-4m4 4l4-4M5 20h14",
  push: "M12 20V8m0 0l-4 4m4-4l4 4M5 4h14",
  branch: "M6 3v12m0 0a3 3 0 103 3 3 3 0 00-3-3zm12-6a3 3 0 10-3-3 3 3 0 003 3zm0 0c0 5-6 3-9 6",
  merge: "M6 9v12M6 9a3 3 0 100-6 3 3 0 000 6zm0 0c0 5 4 9 9 9m0 0a3 3 0 106 0 3 3 0 00-6 0z",
  stash: "M4 7h16v4H4zM6 11v8h12v-8M10 15h4",
  tag: "M3 12V4h8l10 10-8 8L3 12zm5-5h.01",
  terminal: "M4 5h16v14H4zM8 10l3 2-3 2M13 15h3",
  folder: "M3 6h6l2 2h10v11H3z",
  star: "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1-5.4-2.9-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z",
  refresh: "M20 11a8 8 0 10-2.3 6M20 4v7h-7",
  plus: "M12 5v14M5 12h14",
  x: "M6 6l12 12M18 6L6 18",
  check: "M5 12l5 5L20 7",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6",
  chevron: "M9 6l6 6-6 6",
  files: "M7 3h7l5 5v13H7zM14 3v5h5",
  history: "M12 7v5l3 2M4 12a8 8 0 108-8 8 8 0 00-6 2.7M4 4v4h4",
  user: "M12 4a4 4 0 100 8 4 4 0 000-8zM4 20c0-4 3.6-6 8-6s8 2 8 6",
  search: "M11 4a7 7 0 100 14 7 7 0 000-14zm9 16l-4-4",
  undo: "M9 14L4 9l5-5M4 9h10a6 6 0 010 12h-3",
  warn: "M12 3l10 18H2zM12 10v5M12 18h.01",
  submodule: "M4 4h7v7H4zM13 13h7v7h-7zM11 7.5h4a2 2 0 012 2V13",
  cloud: "M7 18a4 4 0 01-.5-8 5.5 5.5 0 0110.6-1A4.5 4.5 0 0117 18z",
  command: "M9 9V6a3 3 0 10-3 3h12a3 3 0 10-3-3v12a3 3 0 103-3H6a3 3 0 103 3z",
  sun: "M12 8a4 4 0 100 8 4 4 0 000-8zM12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M5 19l1.5-1.5M17.5 6.5L19 5",
  copy: "M8 8h12v12H8zM4 4h12v2M4 4v12h2",
  down: "M6 9l6 6 6-6",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  gear: "M12 15a3 3 0 100-6 3 3 0 000 6zM19.4 15a1.7 1.7 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.8-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.8 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.8.3H9a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.8V9a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z",
  help: "M12 21a9 9 0 100-18 9 9 0 000 18zM9.5 9.5a2.5 2.5 0 114 2c-.9.6-1.5 1.1-1.5 2.2M12 17h.01",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="icon"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
