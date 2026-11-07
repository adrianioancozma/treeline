import { open } from "@tauri-apps/plugin-dialog";
import { errorMessage } from "../api";
import { useApp } from "../store";
import { useUi } from "../uiStore";
import { Icon } from "./Icon";

export function pickAndOpenRepo(openRepo: (p: string) => Promise<void>, onError: (m: string) => void) {
  return open({ directory: true, multiple: false, title: "Open repository" })
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
        <kbd>F1</kbd> user guide · <kbd>Ctrl</kbd>+<kbd>K</kbd> command palette · <kbd>Ctrl</kbd>+<kbd>T</kbd> new tab · <kbd>Ctrl</kbd>+<kbd>W</kbd> close tab ·{" "}
        <kbd>Ctrl</kbd>+<kbd>Tab</kbd> next tab · <kbd>Ctrl</kbd>+<kbd>O</kbd> open
      </p>
    </div>
  );
}
