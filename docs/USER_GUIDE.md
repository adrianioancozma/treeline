# APMGIT CLIENT — User Guide

APMGIT CLIENT is a desktop Git client. It works with **any Git server** (self-hosted, GitLab, Gitea, Azure DevOps, a
shared folder …) because it simply drives the `git` program installed on your computer. It never asks for or stores
your passwords: sign-in is handled by your normal Git setup (see [Signing in](#signing-in)).

**Contents**
1. [Before you start](#1-before-you-start)
2. [Opening and cloning repositories](#2-opening-and-cloning-repositories)
3. [Tabs and bookmarks](#3-tabs-and-bookmarks)
4. [A tour of the window](#4-a-tour-of-the-window)
5. [Everyday workflow: change, stage, commit, push](#5-everyday-workflow)
6. [History and the commit graph](#6-history-and-the-commit-graph)
7. [Branches and tags](#7-branches-and-tags)
8. [Fetch, pull and push](#8-fetch-pull-and-push)
9. [Submodules](#9-submodules)
10. [Merging, rebasing and conflicts](#10-merging-rebasing-and-conflicts)
11. [Stash: put work aside](#11-stash-put-work-aside)
12. [Undoing things safely](#12-undoing-things-safely)
13. [When something goes wrong](#13-when-something-goes-wrong)
14. [Keyboard shortcuts](#14-keyboard-shortcuts)
15. [Tips for speed](#15-tips-for-speed)
16. [Glossary](#16-glossary)

---

## 1. Before you start

* **Git must be installed** (version 2.30 or newer) and available on your `PATH`. Check by running `git --version` in a
  terminal. Windows: download from <https://git-scm.com>. macOS: run `xcode-select --install` in Terminal, or
  `brew install git`. Linux: `sudo apt install git` (or your distribution's equivalent).
  The app also looks in Git's usual install folders on Windows. If it still cannot find Git, it says so and offers
  **Locate Git…**: choose the Git program (`git.exe`, normally in `C:\Program Files\Git\cmd`) and the app remembers it.
  You can change this later in **Settings** (the Settings button on the start page, or the command palette).
* **Tell Git who you are** once per computer, otherwise commits are refused:
  ```
  git config --global user.name  "Your Name"
  git config --global user.email "you@example.com"
  ```

### Signing in
The app uses whatever Git is already set up with:
* **HTTPS remotes** – the first time Git needs a password or token, your operating system's credential manager
  (Git Credential Manager on Windows/macOS) opens a sign-in window and remembers the result.
* **SSH remotes** – your SSH key must be loaded (`ssh-add -l` lists loaded keys) and its public half added to your
  account on the server.

If a fetch/push fails with an authentication error, the app explains it and shows these steps (see
[section 13](#13-when-something-goes-wrong)).

---

## 2. Opening and cloning repositories

| I want to … | Do this |
| --- | --- |
| Open a repository already on my computer | **Open repository…** on the home screen, or `Ctrl+O`, then pick the folder. Any folder inside a repository works; the app finds the top. |
| Download a repository from a server | **Clone…**, paste the URL, choose the destination folder, press **Clone**. A progress bar shows the download. |
| Clone including submodules | Leave **Clone submodules too** ticked (default). |
| Reopen something I used before | Click its card under **Bookmarks** on the home screen, or use the command palette (`Ctrl+K`). |

If a clone succeeds but a *submodule* can't be downloaded (for example no access to it), the repository still opens and a
message explains what to do; the submodule appears greyed-out in the sidebar and can be initialised later.

---

## 3. Tabs and bookmarks

* Each repository opens in its own **tab**. Switching tabs is instant; only the tab you are looking at is refreshed.
* `Ctrl+T` new tab · `Ctrl+W` close tab · `Ctrl+Tab` / `Ctrl+Shift+Tab` next / previous · middle-click a tab to close it.
* Your open tabs are **restored** next time you start the app.
* A small orange dot on a tab means that repository has uncommitted changes.
* **Bookmarks:** click the ☆ next to the repository name to bookmark it (★ = bookmarked). Remove it by clicking ★ again,
  or the ✕ on the bookmark card.

---

## 4. A tour of the window

```
┌ Tabs ─────────────────────────────────────────────────────  ⌘ palette · ☀ theme ┐
│ Repo name · branch chip · ★   [Commit][Fetch][Pull▾][Push] [Branch][Merge][Stash][Tag] [Terminal][Explorer][Refresh] │
├──────────────┬──────────────────────────────────────────────────────────────────┤
│  Sidebar     │  Main area: File status  — or —  History                         │
│  • File status│                                                                  │
│  • History   │                                                                  │
│  • Branches  │                                                                  │
│  • Remotes   │                                                                  │
│  • Tags      │                                                                  │
│  • Stashes   │                                                                  │
│  • Submodules│                                                                  │
├──────────────┴──────────────────────────────────────────────────────────────────┤
│ Status bar: what the app is doing now · repository path                          │
└──────────────────────────────────────────────────────────────────────────────────┘
```

* **Toolbar badges:** the number on *Commit* is how many files changed; on *Push* how many commits you haven't pushed; on
  *Pull* how many commits are waiting on the server.
* **Branch chip** `main ↑1 ↓2` – you are 1 commit ahead of and 2 commits behind the remote branch.
* **Right-click** almost anything (branches, tags, stashes, files, commits, submodules) for its actions.
* **Drag the thin dividers** between panels to resize them; sizes are remembered.
* **Theme:** the ☀ button switches light/dark (or choose “follow system” in the command palette).
* **Command palette** (`Ctrl+K`): type to find any action — Fetch, New branch, Check out *x*, Open bookmark *y*…
* **This guide is built into the app.** Press `F1`, click the **?** button at the top right, or use **User guide** on the
  home screen. It opens in its own window and works offline. Error panels have a **Learn more** button that opens the
  page on fixing errors, and the command palette has shortcuts to the most useful pages.

---

## 5. Everyday workflow

### 5.1 See what changed
Open **File status** (sidebar). Files are grouped:
* **Conflicts** – needs your decision (see section 10).
* **Staged** – will be included in the next commit.
* **Unstaged** – changed but not yet staged (includes new untracked files, marked **U**).

Click a file to see its changes on the right. Colours: green = added, red = removed.
Badges: **M** modified, **A** added, **D** deleted, **R** renamed, **U** untracked, **!** conflict.

### 5.2 Stage and unstage
* Click **+** next to a file to stage it (or **×** to unstage). Double-click a row for the same effect.
* **Stage all / Unstage all** buttons are in each section header.
* **Stage only part of a file:** with the file open in the diff,
  * click **Stage hunk** to stage one block of changes, or
  * tick the checkboxes beside individual changed lines and click **Stage N lines**.
  This lets you split one messy edit into several clean commits. (In a hunk that touches the very end of a file lacking a
  final newline, only whole-hunk staging is offered, to avoid corrupting the file.)
* **Discard changes** (↶ icon or right-click → *Discard changes…*) throws a file's unstaged changes away. This **cannot be
  undone**; the app asks first.

### 5.3 Commit
1. Write a message in the box under the lists. Aim for a short first line (the counter turns amber past 72 characters).
2. Press **Commit** or `Ctrl+Enter`.
* **Amend last commit** – tick the box to rewrite the previous commit with the currently staged changes and/or a new
  message. Don't amend a commit you've already pushed and shared.
* **Commit & push…** commits, then opens the Push dialog.

### 5.4 Push your work
Press **Push**. The dialog lets you choose the remote and branch. Tick **Track this remote branch** the first time you push
a new branch. **Force push (with lease)** overwrites the remote branch only if nobody else has pushed since you last
fetched — use it after rewriting history (rebase/amend), never casually.

---

## 6. History and the commit graph

Open **History** (sidebar).
* Each row is a commit: graph lines on the left, then message, author, date, short ID. Coloured tags are branches
  (green), remote branches (purple), tags (amber) and where you are (blue = current branch).
* The top row **Uncommitted changes** appears when you have local changes; click it to jump to File status.
* Click a commit to see its details and the files it changed; click a file to see that file's diff.
* **Search** box: finds commits whose *message* contains your text (plain text, not a pattern). Untick **All branches** to
  show only the current branch's history.
* **Highlight a branch:** click a branch, remote branch or tag in the sidebar — the graph scrolls to its tip and
  highlights every commit reachable from it; everything else is dimmed. Click the **✕** on the “Highlighting …” chip to
  clear it.
* **Right-click a commit** for: copy ID, check out, new branch/tag here, cherry-pick, revert, merge into current, reset the
  current branch to here (soft / mixed / hard), and interactive rebase from here.
* Long histories load in pages as you scroll.

---

## 7. Branches and tags

* **Create a branch:** toolbar **Branch** (or right-click a commit → *New branch here…*). Leave *Start from* empty to branch
  from where you are. Tick *Check out* to switch to it immediately.
* **Switch branch:** double-click it in the sidebar (or right-click → *Check out*). Remote branches are checked out as local
  tracking branches. If you have uncommitted changes that would be overwritten, the app tells you and offers to stash.
* **Rename / delete:** right-click the branch. Deleting a branch that isn't fully merged requires ticking the box
  *Delete even if it is not fully merged*. Remote branches can be deleted from the server via *Delete from server…*.
* **Tags:** toolbar **Tag** (or right-click a commit). Add a message to create an *annotated* tag. Right-click a tag to push
  it to the server or delete it (locally).

---

## 8. Fetch, pull and push

| Action | What it does | Where |
| --- | --- | --- |
| **Fetch** | Downloads new commits from all remotes **without changing your files**. Safe any time. | Toolbar; right-click a branch → *Fetch* (just that branch) |
| **Pull** | Fetch **and** bring the remote commits into your current branch. | Toolbar (merge); the ▾ menu adds *rebase* and *fast-forward only*; right-click a branch → *Pull* |
| **Push** | Uploads your commits. | Toolbar; right-click a branch → *Push* |

Notes:
* *Pull (merge)* creates a merge commit if both sides changed; *Pull (rebase)* replays your commits on top of the remote
  ones for a straight history (your uncommitted changes are stashed and restored automatically).
* **Pull on a branch you haven't checked out** can only **fast-forward** it. If it has diverged, Git refuses and the app
  explains why — check the branch out and pull with merge or rebase.
* A branch with no upstream can be pushed (the app sets the upstream for you) but cannot be fetched/pulled until it has one.
* Progress for long operations appears in the status bar.

---

## 9. Submodules

A submodule is another repository embedded inside this one (the sidebar's **Submodules** list).
* Dot colours: green = checked out at the expected commit · **amber** = it is at a *different* commit than the parent
  expects · grey = not downloaded yet · red = in conflict.
* **Open one:** double-click it (or right-click → *Open in new tab*). It opens as its own tab **directly to the right of its
  parent's tab**, where you can commit, branch and push as with any repository.
* A grey (not-initialised) submodule is downloaded for you when you open it (*Initialise & open*).
* **Update all submodules** (right-click any submodule, or the palette) checks out every submodule at the commit the parent
  expects.
* After you commit inside a submodule, the **parent** shows the submodule as changed (it now points to a new commit).
  Stage and commit that in the parent to record the new version.

---

## 10. Merging, rebasing and conflicts

### Merge
Toolbar **Merge** (or right-click a branch → *Merge into …*). Options: always create a merge commit (`--no-ff`), or squash
everything into one uncommitted change.

### Rebase
Right-click a branch → *Rebase current onto this…* replays your commits on top of another branch for a linear history.
Only rebase commits you haven't shared.

### Interactive rebase (tidy up recent commits)
Right-click a commit in History → **Interactive rebase from here…**. For each commit choose **Pick**, **Reword** (edit the
message), **Edit** (pause to amend it), **Squash** (merge into the one before, combining messages), **Fixup** (merge,
discarding its message) or **Drop**. Use the arrows to reorder (newest on top). Uncommitted changes are stashed and restored
automatically. Merge commits aren't listed (a rebase flattens them).

### Conflicts
A conflict means both sides changed the same lines. The operation **pauses** — nothing is lost.
1. A yellow banner (“Merge in progress …”) appears in File status; conflicted files are listed under **Conflicts**.
2. Click a conflicted file. For each conflict choose **Use ours**, **Use theirs**, **Both** or **Both (reversed)** — or edit
   the **Result** box by hand.
3. Click **Mark resolved** (enabled when every conflict is decided). Repeat for each file.
4. When no conflicts remain, press **Continue** in the banner. **Abort** cancels and restores the state from before.

*Ours* = the branch you are on; *theirs* = the one being merged in (for a rebase these are swapped in meaning, which is
normal Git behaviour). If one side deleted the file you get **Keep ours / Keep theirs** instead.

### Cherry-pick, revert, reset
* **Cherry-pick** copies one commit onto the current branch.
* **Revert** makes a *new* commit that undoes an earlier one (safe for shared history).
* **Reset** moves the branch pointer — see section 12.

---

## 11. Stash: put work aside

Stash saves your uncommitted changes and gives you a clean working tree — handy before switching branches.
* **Stash** (toolbar): optionally add a message; *Include untracked files* is on by default.
* Stashes are listed in the sidebar. **Click** one to inspect it in History. **Right-click → Apply** restores it and keeps
  the stash; **Pop** restores and removes it; **Drop** deletes it.

---

## 12. Undoing things safely

| I want to … | Use | Safe? |
| --- | --- | --- |
| Unstage a file | **×** beside it | ✅ |
| Throw away edits to a file | Discard | ⚠ permanent |
| Fix the last commit's message or contents | **Amend last commit** | ✅ if not pushed |
| Undo a pushed commit | **Revert commit** | ✅ |
| Move the branch back to an earlier commit | Reset **soft** (keeps changes staged) / **mixed** (keeps changes unstaged) | ✅ |
| Move back and throw away all changes | Reset **hard** | ⚠ discards uncommitted work |
| Cancel a merge/rebase in progress | **Abort** in the banner | ✅ |
| Get back a commit that “disappeared” | Run `git reflog` in a terminal (the **Terminal** button opens one) and note the ID, then create a branch from it | ✅ |

---

## 13. When something goes wrong

Whenever an operation fails, the app shows a red panel with:
1. a plain-language **title** and explanation of *why* it happened,
2. **How to fix it** — numbered steps,
3. one-click **action buttons** where possible (Pull, Pull (rebase), Stash my changes…, Show changes, Remove lock file),
4. **Technical details** (Git's exact output — expand it when asking for help), **Copy details**, and **Learn more**, which
   opens this page of the guide.

Errors stay on screen until you dismiss them (✕, or **Dismiss all**), and a panel closes itself once you use its fix.

Common situations:

| You see … | Meaning | Fix |
| --- | --- | --- |
| *Git is locked by another process* | A `.lock` file exists — another Git tool is busy, or one crashed | Wait/close other tools; if none, **Remove lock file** |
| *The remote has commits you don't have* | Push rejected (non-fast-forward) | **Pull** (or **Pull (rebase)**), then push again |
| *Git could not sign in to the remote* | Wrong/expired credentials or SSH key | See [Signing in](#signing-in) |
| *Could not reach the remote server* | Offline, VPN or proxy | Check connection; retry |
| *Uncommitted changes are in the way* | A checkout/merge would overwrite your edits | **Stash my changes…**, repeat, then pop |
| *The changes conflict and need your decision* | Merge/rebase conflict | Resolve in File status (section 10) |
| *Git doesn't know who you are* | No `user.name`/`user.email` | Run the two `git config` commands in section 1 |
| *A Git hook stopped the operation* | The repo's pre-commit/pre-push check failed | Read the technical details, fix, retry |
| *This branch isn't linked to a remote branch yet* | No upstream | Push with **Track this remote branch** |
| *Git is not installed (or not on PATH)* | The app could not start Git — programs opened from the Start menu or Finder do not always see your terminal's `PATH` | **Locate Git…** and pick `git.exe`, or install Git (section 1) |
| *A file or folder is in use or protected* | Another program (editor, antivirus, OneDrive) holds a file | Close it; move the repo out of OneDrive |

If the **same** problem keeps appearing, press **Refresh** (`Ctrl+R`), and try the command in the **Terminal** to see Git's
full output.

---

## 14. Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `F1` (or `Ctrl`/`Cmd`+`?`) | Open this user guide |
| `Ctrl+K` | Command palette |
| `Ctrl+O` | Open repository |
| `Ctrl+T` / `Ctrl+W` | New tab / close tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+R` | Refresh |
| `Ctrl+Enter` (in the message box) | Commit |
| `Enter` / `Esc` (in a dialog) | Confirm / cancel |
| Double-click a file | Stage / unstage it |
| Double-click a branch | Check it out |
| Double-click a submodule | Open it in a tab beside its parent |

(On macOS use `Cmd` instead of `Ctrl`, except `Ctrl+Tab` for switching tabs. On a Mac, `F1` may need the `Fn` key, so `Cmd+?` is the easier way to open the guide.)

---

## 15. Tips for speed

* **Keep repositories out of OneDrive / Dropbox / network drives.** Sync tools lock and rescan thousands of small files
  inside `.git` and `node_modules`, which makes every Git tool slow. A plain folder such as `C:\dev` is much faster.
* The app refreshes by **watching the files** of the active tab — you don't need to press Refresh after editing in your
  editor.
* Very large histories load 400 commits at a time; keep scrolling to load more.
* Add `node_modules`, `dist` and build output to `.gitignore` so Git doesn't scan them.

---

## 16. Glossary

* **Repository (repo)** – a project folder with its full history (stored in a hidden `.git` folder).
* **Commit** – a saved snapshot with a message.
* **Branch** – a movable label pointing at a commit; lets you work in parallel.
* **HEAD** – “where you are now”: the commit/branch you have checked out.
* **Remote / origin** – a copy of the repository on a server; *origin* is the usual name of the main one.
* **Upstream** – the remote branch a local branch is linked to.
* **Stage / index** – the holding area of changes chosen for the next commit.
* **Working tree** – the actual files on disk.
* **Fetch / pull / push** – download / download-and-integrate / upload commits.
* **Merge / rebase** – two ways of combining branches (merge commit vs. replaying commits).
* **Conflict** – overlapping edits Git can't combine automatically.
* **Stash** – a shelf for temporarily saving uncommitted work.
* **Submodule** – a repository nested inside another.
* **Fast-forward** – moving a branch forward along a straight line of commits, no merge needed.
