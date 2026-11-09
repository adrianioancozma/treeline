import { useMemo, useState, type ReactNode } from "react";
import { repoActions } from "../actions";
import { parseTrack } from "../lib/format";
import { buildRefTree, type RefFolder } from "../lib/refTree";
import { useApp } from "../store";
import { openMenu, setSectionOpen, useUi } from "../uiStore";
import type { RefInfo, Tab } from "../types";
import { Icon } from "./Icon";

/**
 * Open/closed state of a sidebar section: the user's own choice in this tab (each tab keeps its
 * own layout), else the given default.
 */
function useSection(tabId: string, key: string, fallback: boolean) {
  const stored = useUi((s) => s.sections[tabId]?.[key]);
  const open = stored ?? fallback;
  // Fires for the user's click and when the `open` prop changes; only a real difference is a choice.
  const onToggle = (e: { currentTarget: HTMLDetailsElement }) => {
    if (e.currentTarget.open !== open) setSectionOpen(tabId, key, e.currentTarget.open);
  };
  return { open, onToggle };
}

function Section({ tabId, title, count, children, open: fallback = true }: { tabId: string; title: string; count?: number; children: ReactNode; open?: boolean }) {
  const { open, onToggle } = useSection(tabId, title, fallback);
  return (
    <details open={open} onToggle={onToggle} className="side-section">
      <summary>
        <Icon name="chevron" size={12} />
        <span>{title}</span>
        {count !== undefined && <span className="count">{count}</span>}
      </summary>
      <div className="side-items">{children}</div>
    </details>
  );
}

function RemoteGroup({ tabId, remote, count, children }: { tabId: string; remote: string; count: number; children: ReactNode }) {
  const { open, onToggle } = useSection(tabId, `remote:${remote}`, true);
  return (
    <details open={open} onToggle={onToggle} className="side-sub">
      <summary>
        <Icon name="cloud" size={14} /> {remote} <span className="count">{count}</span>
      </summary>
      {children}
    </details>
  );
}

/** Pixels of indent per folder level. */
const INDENT = 14;

/**
 * A ref tree: sub-folders first, then the refs sitting directly in this folder. `base` is the left
 * padding of depth 0 and `scope` keeps folder open/closed state apart between sections.
 */
function RefTree<T>({ tabId, scope, folder, depth = 0, base, renderItem }: {
  tabId: string;
  scope: string;
  folder: RefFolder<T>;
  depth?: number;
  base: number;
  renderItem: (item: T, label: string, paddingLeft: number) => ReactNode;
}) {
  return (
    <>
      {folder.folders.map((f) => (
        <FolderNode key={f.path} tabId={tabId} scope={scope} folder={f} depth={depth} base={base} renderItem={renderItem} />
      ))}
      {folder.items.map(({ item, label }) => renderItem(item, label, base + depth * INDENT))}
    </>
  );
}

function FolderNode<T>({ tabId, scope, folder, depth, base, renderItem }: {
  tabId: string;
  scope: string;
  folder: RefFolder<T>;
  depth: number;
  base: number;
  renderItem: (item: T, label: string, paddingLeft: number) => ReactNode;
}) {
  const { open, onToggle } = useSection(tabId, `folder:${scope}:${folder.path}`, true);
  return (
    <details open={open} onToggle={onToggle} className="side-folder">
      <summary style={{ paddingLeft: base - 2 + depth * INDENT }} title={folder.path}>
        <Icon name="chevron" size={12} />
        <Icon name="folder" size={14} />
        <span className="grow ellipsis">{folder.name}</span>
        <span className="count">{folder.count}</span>
      </summary>
      <RefTree tabId={tabId} scope={scope} folder={folder} depth={depth + 1} base={base} renderItem={renderItem} />
    </details>
  );
}

export function RepoSidebar({ tab }: { tab: Tab }) {
  const d = useApp((s) => s.data[tab.id]);
  const setUi = useApp((s) => s.setUi);
  const a = repoActions(tab.id);
  const [filter, setFilter] = useState("");
  const f = filter.toLowerCase();
  const match = (n: string) => !f || n.toLowerCase().includes(f);

  const refs = d?.refs ?? [];
  const branches = refs.filter((r) => r.kind === "branch" && match(r.name));
  const tags = refs.filter((r) => r.kind === "tag" && match(r.name));
  const branchTree = buildRefTree(branches, (b) => b.name);
  const tagTree = buildRefTree(tags, (t) => t.name);
  const remoteGroups = useMemo(() => {
    const m = new Map<string, RefInfo[]>();
    for (const r of refs.filter((r) => r.kind === "remote" && match(r.name))) {
      const remote = r.name.split("/")[0];
      m.set(remote, [...(m.get(remote) ?? []), r]);
    }
    // Inside a remote, `origin/feature/login` is the branch `feature/login`: fold it into folders.
    return [...m.entries()].map(([remote, rs]) => ({
      remote,
      count: rs.length,
      tree: buildRefTree(rs, (r) => r.name.slice(remote.length + 1)),
    }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refs, f]);

  const view = d?.ui.view ?? "files";
  const focusRef = d?.ui.focusRef ?? null;
  /** Single click: show the history and highlight everything reachable from this ref. */
  const focus = (name: string) => setUi(tab.id, { view: "history", focusRef: name });
  const changes = d?.status?.files.length ?? 0;
  const current = d?.status?.branch ?? null;

  return (
    <aside className="repo-side">
      <div className="filter-row">
        <Icon name="search" size={14} />
        <input placeholder="Filter branches, tags…" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filter sidebar" />
      </div>
      <nav className="side-scroll">
        <div className="side-block">
          <button className={`side-item${view === "files" ? " active" : ""}`} onClick={() => setUi(tab.id, { view: "files" })}>
            <Icon name="files" /> <span className="grow">File status</span>
            {changes > 0 && <span className="pill">{changes}</span>}
          </button>
          <button className={`side-item${view === "history" ? " active" : ""}`} onClick={() => setUi(tab.id, { view: "history" })}>
            <Icon name="history" /> <span className="grow">History</span>
          </button>
        </div>

        <Section tabId={tab.id} title="Branches" count={branches.length}>
          <RefTree tabId={tab.id} scope="branch" folder={branchTree} base={10} renderItem={(b, label, pad) => {
            const t = parseTrack(b.track);
            const isCurrent = b.head;
            return (
              <button
                key={b.name}
                style={{ paddingLeft: pad }}
                className={`side-item${isCurrent ? " current" : ""}${focusRef === b.name ? " active" : ""}`}
                title={b.upstream ? `${b.name} → ${b.upstream}\nClick to highlight its history, double-click to check out` : b.name}
                onClick={() => focus(b.name)}
                onDoubleClick={() => !isCurrent && a.checkout(b.name)}
                onContextMenu={(e) =>
                  openMenu(e, [
                    { label: "Check out", onClick: () => a.checkout(b.name), disabled: isCurrent },
                    ...((d?.submodules.length ?? 0) > 0
                      ? [{
                          label: isCurrent ? "Switch submodules to this branch" : "Check out + switch submodules",
                          onClick: () => void (isCurrent ? a.switchSubmodules(b.name) : a.checkoutWithSubmodules(b.name)),
                        }]
                      : []),
                    { separator: true },
                    { label: "Fetch", onClick: () => a.fetchBranch(b.name, "branch"), disabled: !b.upstream },
                    { label: isCurrent ? "Pull" : "Pull (fast-forward)", onClick: () => a.pullBranch(b.name), disabled: !b.upstream },
                    { label: "Push", onClick: () => a.pushBranch(b.name) },
                    { separator: true },
                    { label: `Merge into ${current ?? "current"}…`, onClick: () => a.merge(b.name), disabled: isCurrent },
                    { label: `Rebase ${current ?? "current"} onto this…`, onClick: () => a.rebaseOnto(b.name), disabled: isCurrent },
                    { separator: true },
                    { label: "New branch from here…", onClick: () => a.newBranch(b.name) },
                    { label: "Rename…", onClick: () => a.renameBranch(b.name) },
                    { label: "Delete…", danger: true, onClick: () => a.deleteBranch(b.name), disabled: isCurrent },
                  ])
                }
              >
                <Icon name="branch" size={14} />
                <span className="grow ellipsis">{label}</span>
                {t.ahead > 0 && <span className="ahead small">↑{t.ahead}</span>}
                {t.behind > 0 && <span className="behind small">↓{t.behind}</span>}
                {t.gone && <span className="muted small" title="Upstream branch was deleted">gone</span>}
                {isCurrent && <span className="dot" title="Current branch" />}
              </button>
            );
          }} />
          {branches.length === 0 && <p className="muted small pad">No branches.</p>}
        </Section>

        <Section tabId={tab.id} title="Remotes" count={remoteGroups.length}>
          {remoteGroups.map(({ remote, count, tree }) => (
            <RemoteGroup tabId={tab.id} key={remote} remote={remote} count={count}>
              <RefTree tabId={tab.id} scope={`remote:${remote}`} folder={tree} base={34} renderItem={(r, label, pad) => (
                <button
                  key={r.name}
                  style={{ paddingLeft: pad }}
                  className={`side-item${focusRef === r.name ? " active" : ""}`}
                  title={r.name}
                  onClick={() => focus(r.name)}
                  onDoubleClick={() => a.checkoutRemote(r.name)}
                  onContextMenu={(e) =>
                    openMenu(e, [
                      { label: "Check out as local branch", onClick: () => a.checkoutRemote(r.name) },
                      { label: "Fetch", onClick: () => a.fetchBranch(r.name, "remote") },
                      { label: `Merge into ${current ?? "current"}…`, onClick: () => a.merge(r.name) },
                      { separator: true },
                      { label: "Delete from server…", danger: true, onClick: () => a.deleteRemoteBranch(r.name) },
                    ])
                  }
                >
                  <Icon name="branch" size={14} />
                  <span className="grow ellipsis">{label}</span>
                </button>
              )} />
            </RemoteGroup>
          ))}
          {remoteGroups.length === 0 && <p className="muted small pad">No remote branches. Try Fetch.</p>}
        </Section>

        <Section tabId={tab.id} title="Tags" count={tags.length} open={false}>
          <RefTree tabId={tab.id} scope="tag" folder={tagTree} base={26} renderItem={(t, label, pad) => (
            <button
              key={t.name}
              style={{ paddingLeft: pad }}
              className={`side-item${focusRef === t.name ? " active" : ""}`}
              onClick={() => focus(t.name)}
              onDoubleClick={() => a.checkout(t.name)}
              onContextMenu={(e) =>
                openMenu(e, [
                  { label: "Check out (detached)", onClick: () => a.checkout(t.name) },
                  { label: "Push to server", onClick: () => a.pushTag(t.name) },
                  { separator: true },
                  { label: "Delete…", danger: true, onClick: () => a.deleteTag(t.name) },
                ])
              }
            >
              <Icon name="tag" size={14} />
              <span className="grow ellipsis">{label}</span>
            </button>
          )} />
          {tags.length === 0 && <p className="muted small pad">No tags.</p>}
        </Section>

        <Section tabId={tab.id} title="Stashes" count={d?.stashes.length ?? 0} open={(d?.stashes.length ?? 0) > 0}>
          {d?.stashes.map((s) => (
            <button
              key={s.ref}
              className="side-item" style={{ paddingLeft: 26 }}
              title={s.message}
              onClick={() => setUi(tab.id, { view: "history", selCommit: s.ref })}
              onContextMenu={(e) =>
                openMenu(e, [
                  { label: "Apply", onClick: () => a.stashAction(s.ref, "apply") },
                  { label: "Pop (apply and drop)", onClick: () => a.stashAction(s.ref, "pop") },
                  { separator: true },
                  { label: "Drop…", danger: true, onClick: () => a.stashAction(s.ref, "drop") },
                ])
              }
            >
              <Icon name="stash" size={14} />
              <span className="grow ellipsis">{s.message.replace(/^(WIP on|On) /, "")}</span>
            </button>
          ))}
          {(d?.stashes.length ?? 0) === 0 && <p className="muted small pad">No stashes.</p>}
        </Section>

        {(d?.submodules.length ?? 0) > 0 && (
          <Section tabId={tab.id} title="Submodules" count={d!.submodules.length}>
            {d!.submodules.map((m) => (
              <button
                key={m.path}
                className="side-item" style={{ paddingLeft: 26 }}
                title={`${m.path}\nDouble-click to open`}
                onDoubleClick={() => void a.openSubmodule(m)}
                onContextMenu={(e) =>
                  openMenu(e, [
                    { label: m.state === "uninitialized" ? "Initialise & open" : "Open in new tab", onClick: () => void a.openSubmodule(m) },
                    { label: "Update all submodules", onClick: () => a.updateSubmodules() },
                  ])
                }
              >
                <Icon name="submodule" size={14} />
                <span className="grow ellipsis">{m.path}</span>
                <span className={`sub-dot ${m.state}`} title={m.state} />
              </button>
            ))}
          </Section>
        )}
      </nav>
    </aside>
  );
}
