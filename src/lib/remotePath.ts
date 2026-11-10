// The path field of the remote folder picker (like VS Code's): the part up to the last "/" is the folder listed below,
// the rest filters its folders. "~" is the home folder on the other computer.

export interface Folder {
  name: string;
  repo: boolean;
}

/** "~/dev/p-v" → { dir: "~/dev", filter: "p-v" }; an empty field means the home folder. */
export function splitInput(input: string): { dir: string; filter: string } {
  if (input === "" || input === "~" || input === "~/") return { dir: "~", filter: "" };
  const cut = input.lastIndexOf("/");
  if (cut < 0) return { dir: "~", filter: input };
  const dir = cut === 0 ? "/" : input.slice(0, cut);
  return { dir, filter: input.slice(cut + 1) };
}

/** The field's value after going into `name`, listing it next. */
export function intoFolder(dir: string, name: string): string {
  return `${dir.endsWith("/") ? dir : `${dir}/`}${name}/`;
}

/** "/home/a/dev" → "~/dev" when /home/a is the home folder. */
export function withTilde(path: string, home: string | null): string {
  if (!home) return path;
  if (path === home) return "~";
  return path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}

/** Names that start with the text first, then names that contain it; case is ignored. */
export function filterFolders<T extends Folder>(folders: T[], filter: string): T[] {
  if (!filter) return folders;
  const f = filter.toLowerCase();
  const starts = folders.filter((e) => e.name.toLowerCase().startsWith(f));
  const contains = folders.filter((e) => !e.name.toLowerCase().startsWith(f) && e.name.toLowerCase().includes(f));
  return [...starts, ...contains];
}
