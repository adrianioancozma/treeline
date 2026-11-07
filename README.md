# APMGIT CLIENT

A fast desktop Git client with a Sourcetree-style interface. It talks to the system `git` CLI, so it works
with **any Git remote** (self-hosted, GitLab, Gitea, Azure DevOps, a bare repo on a file share …). There are
no provider integrations and no sign-in.

Built with **Tauri 2** (Rust) and **React + TypeScript**. Runs on Windows, macOS and Linux.

> **New here? Read the [User Guide](docs/USER_GUIDE.md)** — a step-by-step walkthrough of every feature, what each error means and how to fix it.

## Features

| Area | What you get |
| --- | --- |
| **Tabs** | Many repositories open at once. Only the active tab is refreshed, and changes are picked up by a file watcher (no polling). Tabs are restored on start-up. |
| **Bookmarks** | Star a repository to keep it one click away on the home tab and in the command palette. |
| **Clone** | Any URL, live progress, optional submodules. A clone whose only failure is a submodule still opens, with a warning. |
| **Submodules** | Shown in the sidebar with status, opened in their own tab (double-click), initialised on demand, bulk update. |
| **Working copy** | Stage/unstage/discard per file, **per hunk, or per line**. Commit, amend, commit & push. |
| **History** | Virtualised commit graph (smooth with 10k+ commits), search, all-branches toggle, uncommitted-changes row, commit detail and per-file diff. |
| **Branches & tags** | Create, check out, rename, delete (local and remote), tag, push tag. |
| **Remotes** | Fetch, pull (merge / rebase / fast-forward only), push (upstream, tags, force-with-lease) with progress. |
| **Merge tools** | Merge (`--no-ff`, squash), rebase, **interactive rebase** (pick/reword/edit/squash/fixup/drop, reorder), cherry-pick, revert, reset, stash, and a **conflict resolver** with per-conflict ours/theirs/both and an editable result. |
| **UX** | Command palette (`Ctrl+K`), context menus, dark/light theme, resizable panes, keyboard shortcuts, and the full user guide built in (`F1`, works offline). |

### Keyboard shortcuts

`Ctrl+K` command palette · `Ctrl+O` open repository · `Ctrl+T` new tab · `Ctrl+W` close tab ·
`Ctrl+Tab` / `Ctrl+Shift+Tab` next / previous tab · `Ctrl+R` refresh · `Ctrl+Enter` commit.

## Why it is fast

* Every Git call runs on a worker thread; the UI never waits on a process.
* A refresh runs `status`, refs, stashes, remotes and submodules **in parallel**. Submodule state comes from one
  `ls-files` plus parallel `rev-parse`, not the slow `git submodule status`.
* Read-only commands run with `GIT_OPTIONAL_LOCKS=0`, so they never rewrite `.git/index` or trigger the watcher.
* The commit list is windowed (only visible rows exist in the DOM) and paged 400 commits at a time.
* Only the **active** tab is watched and refreshed.

## Requirements

* **Git 2.30+** on your `PATH` (the app calls it; it does not bundle Git).
* To build: Node 20+, pnpm, and a Rust toolchain. Platform extras are listed below.

## Develop

```sh
pnpm install
pnpm tauri dev        # run the app with hot reload
pnpm test             # TypeScript unit tests (diff/patch builder, graph, conflicts, formatting)
pnpm exec tsc --noEmit
cd src-tauri && cargo test   # Rust tests, including integration tests against real git
```

## Build installers

Installers must be built on the target OS (Tauri does not cross-compile desktop bundles).

```sh
pnpm tauri build
```

| OS | Extra prerequisites | Output (`src-tauri/target/release/bundle/`) |
| --- | --- | --- |
| Windows | Visual Studio Build Tools (C++), WebView2 | `nsis/*.exe`, `msi/*.msi` |
| macOS | Xcode command line tools | `dmg/*.dmg`, `macos/*.app` |
| Linux | `libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev` | `deb/*.deb`, `appimage/*.AppImage`, `rpm/*.rpm` |

Regenerate the icon set from a 1024×1024 PNG with `pnpm tauri icon path/to/icon.png`.

## Website, documentation and presentation

A dependency-free static site lives in `site/` and can be hosted anywhere (any web server, a file share, or a static
host of your choice) or opened locally:

| Page | What it is |
| --- | --- |
| `site/index.html` | Landing / overview page with screenshots and the feature tour |
| `site/present.html` | Full-screen slide deck (←/→, Space, `F` for full screen, `#5` deep links) |
| `site/docs/*.html` | Documentation, **generated from `docs/USER_GUIDE.md`** (one page per section) |

```sh
pnpm site:build              # regenerate site/docs from docs/USER_GUIDE.md
node scripts/check-site.mjs  # verify every link, image and #anchor in the site
```

Edit the guide (not the generated HTML), rebuild, then commit both. The site is also bundled into the app
(`scripts/sync-guide.mjs` copies it to the git-ignored `public/guide/` before every dev run and build) so **F1** opens it
offline in its own window. Serve locally with any static server, for example
`npx serve site`.

## Project layout

```
docs/USER_GUIDE.md       end-user guide
src/                     React UI
  store.ts               app state (tabs, bookmarks, per-repo data, refresh logic)
  actions.ts             every user action (fetch, push, merge …) shared by toolbar, menus and palette
  lib/diff.ts            unified-diff parser and hunk/line patch builder
  lib/graph.ts           commit-graph lane layout
  lib/conflict.ts        conflict-marker parser and resolver
  components/            views and dialogs
src-tauri/src/
  git.rs                 process helpers, status, submodules, clone
  ops.rs                 staging, commits, history, branches, remotes, merge/rebase, stash, conflicts
  watcher.rs             file-system watcher for the active repo
  store.rs               state persistence
```

## Notes

* Credentials come from your normal Git setup (credential helper, SSH agent). The app never prompts for or
  stores passwords itself, and sets `GIT_TERMINAL_PROMPT=0` so a missing credential fails fast instead of hanging.
* `.git/index.lock` errors are shown as git reports them; remove a stale lock file if a previous git process crashed.
