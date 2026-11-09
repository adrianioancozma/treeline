import { repoActions } from "../actions";
import { useApp } from "../store";
import { openMenu, useUi } from "../uiStore";
import type { Tab } from "../types";
import { Icon, type IconName } from "./Icon";

function Btn({
  icon,
  label,
  onClick,
  badge,
  disabled,
  title,
}: {
  icon: IconName;
  label: string;
  onClick: (e: React.MouseEvent<HTMLButtonElement>) => void;
  badge?: string;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button className="tool" onClick={onClick} disabled={disabled} title={title ?? label}>
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

  return (
    <div className="toolbar">
      <div className="repo-id">
        <strong className="repo-name">{tab.name}</strong>
        <span className="chip" title={s?.upstream ? `Tracking ${s.upstream}` : "No upstream branch"}>
          <Icon name="branch" size={13} /> {s?.branch ?? (s ? "detached HEAD" : "…")}
          {s && s.ahead > 0 && <span className="ahead"> ↑{s.ahead}</span>}
          {s && s.behind > 0 && <span className="behind"> ↓{s.behind}</span>}
        </span>
        <button
          className={`icon-btn star${bookmark ? " on" : ""}`}
          aria-label={bookmark ? "Remove bookmark" : "Bookmark this repository"}
          title={bookmark ? "Remove bookmark" : "Bookmark this repository"}
          onClick={() => (bookmark ? removeBookmark(bookmark.id) : tab.path && addBookmark(tab.path))}
        >
          <Icon name="star" size={16} />
        </button>
      </div>

      <div className="tools">
        <Btn icon="files" label="Commit" badge={changes ? String(changes) : undefined} title="Show changes and commit" onClick={() => setUi(tab.id, { view: "files" })} />
        <span className="sep" />
        <Btn icon="fetch" label="Fetch" disabled={busy} onClick={() => a.fetch()} />
        <Btn
          icon="pull"
          label="Pull"
          badge={s?.behind ? String(s.behind) : undefined}
          disabled={busy}
          title="Pull (merge). Right-click for more options"
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
        <Btn icon="branch" label="Branch" onClick={() => a.newBranch()} />
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
        <Btn icon="merge" label="Merge" onClick={() => a.merge()} />
        <Btn icon="merge" label="Merge to many" title="Merge one branch into several branches at once, with a conflict check first" onClick={() => useUi.setState({ multiMerge: { tabId: tab.id } })} />
        <Btn icon="stash" label="Stash" onClick={() => a.stash()} />
        <Btn icon="tag" label="Tag" onClick={() => a.newTag()} />
        <Btn icon="trash" label="Cleanup" title="Find merged branches that are safe to delete" onClick={() => useUi.setState({ mergedBranches: { tabId: tab.id } })} />
        <span className="sep" />
        <Btn icon="terminal" label="Terminal" onClick={() => a.terminal()} />
        <Btn icon="folder" label="Explorer" onClick={() => a.reveal()} />
        <Btn icon="refresh" label="Refresh" disabled={d?.loading} onClick={() => void reload(tab.id)} />
      </div>
    </div>
  );
}
