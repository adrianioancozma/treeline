import { api, errorMessage } from "../api";
import { useApp } from "../store";
import { useUi } from "../uiStore";
import { Icon } from "./Icon";
import { Shortcut } from "./Shortcut";

export function pickAndOpenRepo(openRepo: (p: string) => Promise<void>, onError: (m: string) => void) {
  return api
    .pickFolder("Open repository", "repo")
    .then((dir) => (typeof dir === "string" ? openRepo(dir) : undefined))
    .catch((e) => onError(errorMessage(e)));
}

export function Home() {
  const { bookmarks, openRepo, removeBookmark, notify } = useApp();
  const onError = (m: string) => notify("error", m);
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
        </div>
      </div>

      {bookmarks.length > 0 ? (
        <section className="home-section">
          <h2>Bookmarks</h2>
          <div className="cards">
            {bookmarks.map((b) => (
              <div key={b.id} className="card-wrap">
                <button className="card" onClick={() => openRepo(b.path).catch((e) => onError(errorMessage(e)))}>
                  <strong>
                    <Icon name="star" size={14} /> {b.name}
                  </strong>
                  <span className="muted small">{b.path}</span>
                </button>
                <button className="icon-btn card-x" aria-label={`Remove bookmark ${b.name}`} onClick={() => removeBookmark(b.id)}>
                  <Icon name="x" size={14} />
                </button>
              </div>
            ))}
          </div>
        </section>
      ) : (
        <p className="muted small center-text">Open a repository and press ★ to bookmark it for quick access.</p>
      )}

      <p className="muted small hint center-text">
        <Shortcut spec="F1" /> user guide · <Shortcut spec="Mod+K" /> command palette · <Shortcut spec="Mod+T" /> new tab ·{" "}
        <Shortcut spec="Mod+W" /> close tab · <Shortcut spec="Ctrl+Tab" /> next tab · <Shortcut spec="Mod+O" /> open
      </p>
    </div>
  );
}
