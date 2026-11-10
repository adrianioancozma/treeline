import { useRef } from "react";
import { repoActions } from "../actions";
import { useElementWidth } from "../hooks";
import { fileManagerName, toolbarLayout } from "../lib/toolbarLayout";
import { useApp } from "../store";
import { openMenu, useUi } from "../uiStore";
import type { Tab } from "../types";
import { shortHost } from "./TabBar";
import { Icon, type IconName } from "./Icon";

function Btn({
  icon,
  label,
  onClick,
  badge,
  disabled,
  title,
  keep,
}: {
  icon: IconName;
  label: string;
  onClick: (e: React.MouseEvent<HTMLButtonElement>) => void;
  badge?: string;
  disabled?: boolean;
  title?: string;
  /** Stays on a remote (read-only) tab. */
  keep?: boolean;
}) {
  return (
    <button className={`tool${keep ? " keep" : ""}`} onClick={onClick} disabled={disabled} title={title ?? label}>
      <span className="tool-icon">
        <Icon name={icon} size={18} />
        {badge && <span className="tool-badge">{badge}</span>}
      </span>
      <span className="tool-label">{label}</span>
    </button>
  );
}

export function Toolbar({ tab }: { tab: Tab }) {
  const d = useApp((s) => s.data[tab.id]);
  const bookmarks = useApp((s) => s.bookmarks);
  const { addBookmark, removeBookmark, reload, setUi } = useApp.getState();
  const a = repoActions(tab.id);
  const s = d?.status;
  const busy = !!d?.busy;
  const bookmark = bookmarks.find((b) => b.path === tab.path);
  const changes = s?.files.length ?? 0;
  const ref = useRef<HTMLDivElement>(null);
  const layout = toolbarLayout(useElementWidth(ref));
  const shown = (action: "branch" | "merge" | "stash" | "tag") => !layout.overflow.includes(action);

  // Rare actions live in More; actions that no longer fit the toolbar join them at the top (P10).
  const openMore = (e: React.MouseEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const moved = [
      !shown("branch") && { label: "New branch…", onClick: () => a.newBranch() },
      !shown("merge") && { label: "Merge…", onClick: () => a.merge() },
      !shown("stash") && { label: "Stash…", onClick: () => a.stash() },
      !shown("tag") && { label: "New tag…", onClick: () => a.newTag() },
    ].filter((x): x is { label: string; onClick: () => void } => !!x);
    openMenu({ clientX: r.right - 220, clientY: r.bottom + 4 }, [
      ...moved,
      ...(moved.length ? [{ separator: true as const }] : []),
      { label: "Merge into several branches…", onClick: () => useUi.setState({ multiMerge: { tabId: tab.id } }) },
      { label: "Clean up merged branches…", onClick: () => useUi.setState({ mergedBranches: { tabId: tab.id } }) },
      { separator: true },
      { label: "Open in Terminal", onClick: () => a.terminal() },
      { label: `Show in ${fileManagerName()}`, onClick: () => a.reveal() },
    ]);
  };

  return (
    <div className={`toolbar${layout.labels ? "" : " icons-only"}${tab.host ? " remote" : ""}`} ref={ref}>
      <div className="repo-id">
        <strong className="repo-name">{tab.name}</strong>
        {tab.host && (
          <span className="host-badge" title={`${tab.host}:${tab.path}`}>
            ⇄ {shortHost(tab.host)}
          </span>
        )}
        <span className="chip" title={s?.upstream ? `Tracking ${s.upstream}` : "No upstream branch"}>
          <Icon name="branch" size={13} /> {s?.branch ?? (s ? "detached HEAD" : "…")}
          {s && s.ahead > 0 && <span className="ahead"> ↑{s.ahead}</span>}
          {s && s.behind > 0 && <span className="behind"> ↓{s.behind}</span>}
        </span>
        {!tab.host && (
          <button
            className={`icon-btn star${bookmark ? " on" : ""}`}
            aria-label={bookmark ? "Remove bookmark" : "Bookmark this repository"}
            title={bookmark ? "Remove bookmark" : "Bookmark this repository"}
            onClick={() => (bookmark ? removeBookmark(bookmark.id) : tab.path && addBookmark(tab.path))}
          >
            <Icon name="star" size={16} />
          </button>
        )}
      </div>

      <div className="tools">
        {tab.host && (
          <span className="remote-note" title="Repositories on another computer can only be read in this version.">
            Read-only for now
          </span>
        )}
        <Btn icon="files" label="Commit" badge={changes ? String(changes) : undefined} title="Show changes and commit" onClick={() => setUi(tab.id, { view: "files" })} />
        <span className="sep" />
        <Btn icon="fetch" label="Fetch" disabled={busy} onClick={() => a.fetch()} />
        <Btn
          icon="pull"
          label="Pull"
          badge={s?.behind ? String(s.behind) : undefined}
          disabled={busy}
          title="Pull (merge). The arrow next to it offers rebase and fast-forward only."
          onClick={(e) => {
            if (e.shiftKey) return;
            a.pull("merge");
          }}
        />
        <button
          className="tool mini"
          aria-label="More pull options"
          disabled={busy}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            openMenu({ clientX: r.left - 40, clientY: r.bottom + 4 }, [
              { label: "Pull — merge", onClick: () => a.pull("merge") },
              { label: "Pull — rebase", onClick: () => a.pull("rebase") },
              { label: "Pull — fast-forward only", onClick: () => a.pull("ff-only") },
            ]);
          }}
        >
          <Icon name="down" size={12} />
        </button>
        <Btn icon="push" label="Push" badge={s?.ahead ? String(s.ahead) : undefined} disabled={busy} onClick={() => a.push()} />
        <span className="sep" />
        {shown("branch") && <Btn icon="branch" label="Branch" onClick={() => a.newBranch()} />}
        {(d?.submodules.length ?? 0) > 0 && (
          <Btn
            icon="submodule"
            label="Sync subs"
            disabled={busy || !s?.branch}
            title={s?.branch ? `Switch submodules to ${s.branch} (where that branch exists). The arrow offers Sync and pull` : "Check out a branch first"}
            onClick={() => s?.branch && void a.switchSubmodules(s.branch)}
          />
        )}
        {(d?.submodules.length ?? 0) > 0 && (
          <button
            className="tool mini"
            aria-label="More sync options"
            disabled={busy || !s?.branch}
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              const branch = s?.branch;
              if (!branch) return;
              openMenu({ clientX: r.left - 40, clientY: r.bottom + 4 }, [
                { label: "Sync — switch branches only", onClick: () => void a.switchSubmodules(branch) },
                { label: "Sync and pull — switch, then pull (fast-forward)", onClick: () => void a.switchSubmodules(branch, true) },
              ]);
            }}
          >
            <Icon name="down" size={12} />
          </button>
        )}
        {shown("merge") && <Btn icon="merge" label="Merge" onClick={() => a.merge()} />}
        {shown("stash") && <Btn icon="stash" label="Stash" onClick={() => a.stash()} />}
        {shown("tag") && <Btn icon="tag" label="Tag" onClick={() => a.newTag()} />}
        <span className="sep" />
        <Btn icon="more" label="More" title={`More actions: merge into several branches, clean up, Terminal, ${fileManagerName()}`} onClick={openMore} />
        <Btn icon="refresh" label="Refresh" disabled={d?.loading} onClick={() => void reload(tab.id)} keep />
      </div>
    </div>
  );
}
