/** A folder in a ref tree: `feature/login` and `feature/signup` both live in the folder `feature`. */
export interface RefFolder<T> {
  /** Last path segment, e.g. `feature`. */
  name: string;
  /** Full path from the root, e.g. `release/2024`; unique, so usable as a key. */
  path: string;
  folders: RefFolder<T>[];
  /** Refs sitting directly in this folder; `label` is the last path segment. */
  items: { label: string; item: T }[];
  /** Number of refs in this folder and everything below it. */
  count: number;
}

/**
 * Group refs into folders by the `/` in their names. Folders are sorted by name; refs keep the
 * order they came in. The returned root has an empty name and path.
 */
export function buildRefTree<T>(items: T[], nameOf: (item: T) => string): RefFolder<T> {
  const root: RefFolder<T> = { name: "", path: "", folders: [], items: [], count: 0 };
  for (const item of items) {
    const parts = nameOf(item).split("/");
    let node = root;
    node.count++;
    for (const part of parts.slice(0, -1)) {
      const path = node.path ? `${node.path}/${part}` : part;
      let child = node.folders.find((f) => f.name === part);
      if (!child) {
        child = { name: part, path, folders: [], items: [], count: 0 };
        node.folders.push(child);
      }
      node = child;
      node.count++;
    }
    node.items.push({ label: parts[parts.length - 1], item });
  }
  const sort = (n: RefFolder<T>) => {
    n.folders.sort((a, b) => a.name.localeCompare(b.name));
    n.folders.forEach(sort);
  };
  sort(root);
  return root;
}
