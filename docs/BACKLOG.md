# Backlog

Planned work that is not built yet. Newest first. Move an item out of here (into the user guide and the
changelog) when it ships.

* **Work on a repository on another computer over SSH** (low priority). Run git on the other machine through
  `ssh host git -C <path> …` so the app can open its working tree directly. Today an SSH *remote* plus a local clone
  works; this would add remote execution, polling instead of the file watcher, and handling of latency.

## Shipped

The five items requested in November 2026 are built: history filters (author, commit, date), the Sync subs
drop-down with **Sync and pull**, tab bar scrolling and narrower tabs, double-click a commit to check it out, and
sidebar sections collapsed by default and remembered per tab. They are described in `docs/USER_GUIDE.md`.
