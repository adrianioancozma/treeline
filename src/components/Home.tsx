import { api, errorMessage } from "../api";
import { useApp } from "../store";
import { useUi } from "../uiStore";
import { Icon } from "./Icon";
import { Shortcut } from "./Shortcut";
import { pathParts } from "../lib/format";
import { visibleRecent } from "../lib/recent";

export function pickAndOpenRepo(openRepo: (p: string) => Promise<void>, onError: (m: string) => void) {
  return api
    .pickFolder("Open repository", "repo")
    .then((dir) => (typeof dir === "string" ? openRepo(dir) : undefined))
    .catch((e) => onError(errorMessage(e)));
}

export function Home() {
  const { bookmarks, recent, openRepo, removeBookmark, removeRecent, notify } = useApp();
  const onError = (m: string) => notify("error", m);
  const open = (path: string) => openRepo(path).catch((e) => onError(errorMessage(e)));
  const shownRecent = visibleRecent(recent, bookmarks);
  return (
    <div className="home">
      <div className="home-hero">
        <div className="logo" aria-hidden="true">
          <Icon name="branch" size={30} />
        </div>
        <h1>Treeline</h1>
        <p className="muted">A fast Git client for any provider. Open a repository to get started.</p>
        <div className="row center">
          <button className="btn primary big" onClick={() => pickAndOpenRepo(openRepo, onError)}>
            <Icon name="folder" /> Open repository…
          </button>
          <button className="btn big" onClick={() => useUi.setState({ clone: true })}>
            <Icon name="cloud" /> Clone…
          </button>
          <button className="btn big" onClick={() => useUi.setState({ remoteBrowser: true })}>
            <Icon name="terminal" /> Open remote…
          </button>
        </div>
      </div>

      {bookmarks.length > 0 && (
        <section className="home-section">
          <h2>Bookmarks</h2>
          <div className="cards">
            {bookmarks.map((b) => (
              <RepoCard key={b.id} name={b.name} path={b.path} starred onOpen={open} onRemove={() => removeBookmark(b.id)} removeLabel={`Remove bookmark ${b.name}`} />
            ))}
          </div>
        </section>
      )}

      {shownRecent.length > 0 && (
        <section className="home-section">
          <h2>Recent</h2>
          <div className="cards">
            {shownRecent.map((r) => (
              <RepoCard key={r.path} name={r.name} path={r.path} onOpen={open} onRemove={() => removeRecent(r.path)} removeLabel={`Remove ${r.name} from Recent`} />
            ))}
          </div>
        </section>
      )}

      {bookmarks.length === 0 && (
        <p className="muted small center-text">Open a repository and press ★ to bookmark it for quick access.</p>
      )}

      <p className="muted small hint center-text">
        <Shortcut spec="F1" /> user guide · <Shortcut spec="Mod+K" /> command palette · <Shortcut spec="Mod+T" /> new tab ·{" "}
        <Shortcut spec="Mod+W" /> close tab · <Shortcut spec="Ctrl+Tab" /> next tab · <Shortcut spec="Mod+O" /> open
      </p>
    </div>
  );
}

/** A repository on Home: name, path shortened in the middle, and × to take it off the list. */
function RepoCard(props: { name: string; path: string; starred?: boolean; onOpen: (path: string) => void; onRemove: () => void; removeLabel: string }) {
  const { head, tail } = pathParts(props.path);
  return (
    <div className="card-wrap">
      <button className="card" onClick={() => props.onOpen(props.path)}>
        <strong>
          {props.starred && (
            <>
              <span className="star on">
                <Icon name="star" size={14} />
              </span>{" "}
            </>
          )}
          {props.name}
        </strong>
        {/* The end of the path (repository and its parent folder) stays whole; the start is shortened. */}
        <span className="muted small path-mid" title={props.path}>
          <span className="path-head">{head}</span>
          <span className="path-tail">{tail}</span>
        </span>
      </button>
      <button className="icon-btn card-x" aria-label={props.removeLabel} title={props.removeLabel} onClick={props.onRemove}>
        <Icon name="x" size={14} />
      </button>
    </div>
  );
}
