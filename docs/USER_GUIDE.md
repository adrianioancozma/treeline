# Treeline — User Guide

Treeline is a desktop Git client. It works with **any Git server** (self-hosted, GitLab, Gitea, Azure DevOps, a
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

### Installing Treeline
Download the installer for your system from the **[latest release](https://github.com/suhailaleryani/treeline/releases/latest)**
(open **Assets** at the bottom of the page), then follow the steps for your system. Install [Git](#git-requirement) first
if you do not have it.

**Windows 10 or 11**
1. Download the `…-setup.exe` file (or the `.msi` if your organisation prefers MSI packages).
2. Run it and follow the prompts. Treeline appears in the Start menu.
3. The installers are not code-signed yet, so Windows SmartScreen may say *Unknown publisher*. Choose **More info → Run anyway**.

**macOS 12 or newer**
1. Download the `.dmg` for your Mac: `aarch64` for Apple Silicon (M1 and later), `x64` for Intel.
2. Open it and drag **Treeline** into **Applications**.
3. The app is not notarised yet, so macOS may refuse to open it. Right-click the app, choose **Open**, then **Open** again.
   If it says the app is *damaged*, run `xattr -dr com.apple.quarantine /Applications/Treeline.app` in Terminal.

**Linux**
* **AppImage** (any distribution): download the `.AppImage`, then `chmod +x Treeline*.AppImage` and run it.
* **Debian, Ubuntu**: `sudo apt install ./treeline_*.deb`
* **Fedora, openSUSE**: `sudo dnf install ./treeline-*.rpm`
* You need WebKitGTK 4.1 (`libwebkit2gtk-4.1-0`), which the `.deb` and `.rpm` pull in automatically.

> **Linux and Intel Macs are not tested yet.** Windows 10 and Apple Silicon Macs are the verified platforms. If something
> does not work on your system, please tell us (see *Help us test* in the project's README).

Once installed, the app updates itself: it tells you when a new version is out (see *Version and updates* below).
If you would rather build it yourself, see the README's *Build installers* section.

### Git requirement
* **Git must be installed** (version 2.30 or newer) and available on your `PATH`. Check by running `git --version` in a
  terminal. Windows: download from <https://git-scm.com>. macOS: run `xcode-select --install` in Terminal, or
  `brew install git`. Linux: `sudo apt install git` (or your distribution's equivalent).
  The app also looks in Git's usual install folders on Windows. If it still cannot find Git, it says so and offers
  **Locate Git…**: choose the Git program (`git.exe`, normally in `C:\Program Files\Git\cmd`) and the app remembers it.
  You can change this later in **Settings** (the gear icon in the top bar, or the command palette).
* **Version and updates.** The version number is at the bottom right of the window and in **Settings**. The app checks
  for a newer version a few seconds after it starts, and again every few hours. When one is out, a small **blue dot appears on
  the Settings (gear) button** in the top bar; hover it to see the version number. Open **Settings** to read what is new and
  press **Install and restart**. You can also check by hand with **Settings → Check for updates**. Nothing is downloaded or
  installed until you press **Install and restart**, and if you are offline the check just does nothing.
* **Keep Git up to date.** Treeline runs your Git, so it is only as safe as that Git. If yours is older than the July 2025
  security fixes for cloning and submodules, a notice appears once at start-up. Update Git (see above) before cloning or
  updating submodules of a repository you do not fully trust.
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

### Opening repositories safely

A folder handed to you with its own `.git` can carry settings or scripts that make Git run programs on your computer.
Treeline protects you in a few ways:

* **Only folders you opened.** Treeline works only on folders you chose with **Open repository…** or **Clone…**, folders
  you opened before (they come back when you restart), and folders inside those (a submodule, for example). Anything else is refused with
  *“That folder has not been opened in Treeline.”* A whole drive cannot be opened as a repository.
* **“This repository can run programs.”** The first time you open a folder whose own settings or hooks can run programs, a
  window lists what it found: hook scripts, `core.hooksPath` (used by tools such as husky), `core.sshCommand`, filter, merge
  or diff drivers, credential helpers that run a program, signing programs, or an included settings file. Nothing in the
  repository has been run at that point.
  * **Open anyway** opens it and remembers your answer for that folder. Closing the window opens nothing.
  * Open only folders you made yourself or got from a source you trust. Git LFS and ordinary credential helpers
    (`manager`, `osxkeychain`) are normal and do not trigger the window. If the check itself could not run, you are asked
    in the same way.
* **Always off.** For its own Git calls Treeline switches off two settings that would run a program just by looking at a
  repository: `core.fsmonitor` and external diff programs (and text-conversion drivers when showing a diff). In ordinary
  repositories you will notice nothing; in a very large repository that relies on Git's built-in file-system monitor,
  refreshing can be a little slower.
* **Cloning.** A destination you choose with **Browse** is trusted. If you type the destination instead, a window of the
  operating system asks you to confirm *“Clone … into …”* first.

If a clone succeeds but a *submodule* can't be downloaded (for example no access to it), the repository still opens and a
message explains what to do; the submodule appears greyed-out in the sidebar and can be initialised later.

---

## 3. Tabs and bookmarks

* Each repository opens in its own **tab**. Switching tabs is instant; only the tab you are looking at is refreshed.
* `Ctrl+T` new tab · `Ctrl+W` close tab · `Ctrl+Tab` / `Ctrl+Shift+Tab` next / previous · middle-click a tab to close it.
* Your open tabs are **restored** next time you start the app.
* **Drag a tab** left or right to put your tabs in the order you like; the order is remembered. Each tab also remembers which sidebar sections (Branches, Remotes, Tags, Stashes, Submodules) you opened or closed. A repository starts with **every sidebar section collapsed**; open the ones you need and the tab keeps them open, also after a restart.
* **Many tabs:** tabs get narrower as you open more (hover a tab for its full name and folder). When they no longer fit, turn the **mouse wheel** over the tab bar to scroll sideways. The selected tab is always scrolled into view.
* A small orange dot on a tab means that repository has uncommitted changes.
* **Bookmarks:** click the ☆ next to the repository name to bookmark it (★ = bookmarked). Remove it by clicking ★ again,
  or the ✕ on the bookmark card.

---

## 4. A tour of the window

```
┌ Tabs ─────────────────────────────────────────────────────  ⌘ palette · ☀ theme ┐
│ Repo name · branch chip · ★   [Commit][Fetch][Pull▾][Push] [Branch][Sync subs][Merge][Merge to many][Stash][Tag] [Terminal][Explorer][Refresh] │
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
* **Sync subs** appears when the repository has submodules (section 9). Click it to switch the submodules to the current branch; the small arrow beside it offers **Sync** (the same) and **Sync and pull** (switch, then pull each one). **Merge to many** merges one branch into several at
  once, with a conflict check first (section 10).
* **Hover for hints.** The buttons in the conflict resolver (Use ours, Keep theirs, Mark resolved …) explain exactly what
  they do and name the branches involved.
* **Right-click** almost anything (branches, tags, stashes, files, commits, submodules) for its actions.
* **Drag the thin dividers** between panels to resize them; sizes are remembered.
* **Theme:** the ☀ button switches light/dark (or choose “follow system” in the command palette).
* **Command palette** (`Ctrl+K`): type to find any action — Fetch, New branch, Check out *x*, Open bookmark *y*…
* **This guide is built into the app.** Press `F1` or click the **?** button at the top right. It opens in its own window and works offline. Error panels have a **Learn more** button that opens the
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
* Drag the left edge of the **Author**, **Date** or **Commit** column header to resize it (double-click to reset).
* Each row is a commit: graph lines on the left, then message, author, date, short ID. Coloured tags are branches
  (green), remote branches (purple), tags (amber) and where you are (blue = current branch).
* The top row **Uncommitted changes** appears when you have local changes; click it to see the staged and unstaged changes in the panel below (read-only; use the Files view to stage and commit).
* Click a commit to see its details in the panel below the list: the full message, author, date and parents, the **Changed
  files**, and the diff of the selected file. The panel always keeps room, so a tall list can never push it off the screen,
  even in a small window. Drag the divider between the list and the panel to resize it.
* Drag the thin bar between the **message box** and **Changed files** to make the message taller; double-click it to go back
  to automatic height. Both boxes scroll on their own when their content does not fit.
* **Filters** above the list: **Commit** (text in the message, or the start of a commit ID), **Author** (name or e-mail; it
  suggests the authors in the loaded history) and **date**. The date offers quick choices counted back from now (last
  hour, 24 hours, 7 days, 30 days) or **Custom range…** with a *from* and a *to*, each with a date **and a time**. Text is
  matched as plain text in any letter case, and the filters combine (all must match). Commits that do not match are
  dimmed; the graph itself never changes. The summary shows how many commits match, with **▲ ▼** to jump between them
  (it loads more history when needed). Every filter has its own **✕**; the **✕** at the end of the summary clears them
  all. Untick **All branches** to show only the current branch's history.
* **Double-click a commit** to check it out: its branch when it has one (a short list if several), the local copy of a
  remote branch when only that exists, otherwise the commit itself as a detached HEAD, after a confirmation. Uncommitted
  changes that would be overwritten stop the checkout with the usual explanation; nothing is discarded.
* **Highlight a branch:** click a branch, remote branch or tag in the sidebar — the graph scrolls to its tip and
  highlights every commit reachable from it; everything else is dimmed. Click the **✕** on the “Highlighting …” chip to
  clear it.
* **Right-click a commit** for: copy ID, check out, new branch/tag here, cherry-pick, revert, merge into current, reset the
  current branch to here (soft / mixed / hard), and interactive rebase from here.
* Long histories load in pages as you scroll.
* **Compare two commits:** click one commit, then hold `Ctrl` (`Cmd` on a Mac) and click a second one. The panel shows
  what changed *between* them (older → newer), file by file. Ctrl+click the second commit again, or press **Clear**, to
  go back to a single commit.
* **History of one file:** right-click a file (in File status, or in a commit's list of changed files) and choose
  **File history…**. You see every commit that changed that file, even before it was renamed, and the change each one
  made.
* **Who changed each line (Blame):** right-click a file and choose **Blame…**, or switch to the **Blame** tab in the file
  history window. Each line shows who last changed it, when, and in which commit; click that name to jump to the
  commit in the file's history. From a commit's file list, **Blame at this commit…** shows the file as it was then.

---

## 7. Branches and tags

* **Create a branch:** toolbar **Branch** (or right-click a commit → *New branch here…*). Leave *Start from* empty to branch
  from where you are. Tick *Check out* to switch to it immediately.
* **Switch branch:** double-click it in the sidebar (or right-click → *Check out*). Remote branches are checked out as local
  tracking branches. If you have uncommitted changes that would be overwritten, the app tells you and offers to stash.
* **Tracking branch:** right-click a local branch → *Set tracking branch…* to choose which remote branch it follows (or none).
  Checking out a remote branch also makes an existing local branch of the same name track it when it had no upstream.
* **Rename / delete:** right-click the branch. Deleting a branch that isn't fully merged requires ticking the box
  *Delete even if it is not fully merged*. Remote branches can be deleted from the server via *Delete from server…*.
* **Merge one branch into several:** right-click a branch → *Merge this into multiple branches…* (section 10). If the
  repository has submodules, *Check out + switch submodules* also moves them (section 9).
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

### Automatic background fetch

You don't have to press **Fetch** to see what happened on the server. Treeline quietly fetches the repository in the
active tab when you open it, every **2 minutes** (you can change this) while the window is visible, and whenever you switch back to the window.
So a branch merged on GitHub or GitLab (or pushed by a teammate) shows up in the sidebar and the history by itself, and
remote branches deleted on the server disappear.

* It only **fetches** (it never changes your files, your branches or your checked-out work) and it is silent: no progress
  bar and no pop-ups.
* Only the **active tab** is fetched, and repositories with no remote are skipped.
* If you are offline, or your sign-in is missing or expired, the background fetch just does nothing. Press **Fetch** to see
  the real error message. Treeline never opens a password prompt on its own, so for an HTTPS remote run `git fetch` once in
  a terminal and sign in so the credential manager remembers you (see [Signing in](#signing-in)).
* **Change how often, or turn it off:** open **Settings** (the gear in the top bar) → **Background fetch** and choose *every
  15 seconds*, *30 seconds*, *every minute*, *2*, *3*, *5*, *10* or *30 minutes*, or *Never*. The change applies right away
  (the timer restarts and checks at once) and is remembered. Very short intervals use more network and battery, and some
  servers limit how often you may contact them, so 1–2 minutes is a good choice.
* You can still press **Fetch** (or `Ctrl+K` → *Fetch*) at any time to update immediately.

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
* **Switch the submodules to the same branch.** When the submodules have a branch with the same name as the one you use in
  the parent (for example `release-2`), right-click the branch → **Check out + switch submodules**, or **Switch submodules to
  this branch** for the branch that is already checked out, or press **Sync subs** in the toolbar (it uses the current
  branch). The arrow beside the button also offers **Sync and pull**: after switching, each submodule that was switched is
  pulled (fast-forward only, so nothing is ever left half-merged). A submodule without an upstream branch is switched but
  not pulled, and a failed pull is reported while the others carry on.
  * Each downloaded submodule moves to the branch of that name if it exists there, either locally or as a remote branch (a
    local tracking branch is created). Submodules that are not downloaded, or that do not have that branch, are skipped.
  * A submodule that cannot switch (for example because it has uncommitted changes) is reported, and the others still
    switch. Nothing is forced.
  * Treeline does not fetch first. If the branch exists only on the server, **Fetch** inside that submodule.
  * The parent then shows the submodules as changed (amber); commit that to record their new versions.
* After you commit inside a submodule, the **parent** shows the submodule as changed (it now points to a new commit).
  Stage and commit that in the parent to record the new version.

---

## 10. Merging, rebasing and conflicts

### Merge
Toolbar **Merge** (or right-click a branch → *Merge into …*). Options: always create a merge commit (`--no-ff`), or squash
everything into one uncommitted change.

### Merge one branch into several (Merge to many)
Use this when the same change must reach several branches, for example a bug fix that belongs in `release-1`, `release-2`,
`release-3` and `release-4`. You do it once instead of four times, and you see the conflicts **before** anything changes.

1. Toolbar **Merge to many** (or right-click a branch → *Merge this into multiple branches…*).
2. **Choose.** *Merge from* is the current branch unless you change it. Tick the branches that should receive it
   (**Select all** is there too), then **Next: check for conflicts**. Nothing has changed yet.
3. **Review.** Treeline test-merges into every branch without touching your files and shows one result for each:
   *Fast-forward*, *Clean merge*, *Already up to date*, *Conflict* (with the files that conflict) or *Cannot merge* (for
   example the branch is checked out in another worktree). Safe branches are ticked; conflicting ones cannot be ticked.
   Press **Merge into N branches**.
4. **Done.** You get one line per branch. Nothing is pushed: push each branch yourself afterwards.

How the merges are made: a branch that is not checked out is updated directly, without switching to it, with a merge commit
named *Merge branch 'X' into Y*. The branch you have checked out gets a normal merge, so commit or stash your uncommitted
changes first if Git refuses. Every branch is checked again right before merging, and one that has become conflicting is
skipped, never forced.

**A branch with conflicts.** Press **Resolve…** next to it. The dialog hides, Treeline checks that branch out and starts the
merge, and you resolve the conflicts in File status as usual (see *Conflicts* below) and press **Continue**. A bar at the
bottom of the window then offers **Resume**: Treeline re-tests every branch, and the one you finished shows as *Already up to
date*. **Cancel** closes everything.

**Reuse my resolution.** The first time you press **Resolve…**, Treeline turns on Git's *rerere* (“reuse recorded
resolution”) for this repository, which is a setting in its `.git/config`, and tells you so. Git then remembers how you
resolved the conflict. Afterwards, branches that still conflict get **Apply my earlier resolution to N**. For each one
Treeline checks the branch out, lets Git replay your fix, and commits only if every conflict was covered; otherwise it
aborts and leaves the branch untouched and says so. This needs a clean working tree and returns you to the branch you were
on. It works when the conflict is identical, which is typical for the same fix going to several release branches.

**“Merged, but changes dropped.”** If a merge ended up with a branch's own version of a file (for example you chose **Keep
ours** where you should have combined), the change you wanted is missing there. Treeline checks this when you press
**Resume** and after replaying a resolution, and shows a red banner naming the branch and files. To fix it, **Revert** that
merge commit and merge again.

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

**Hover any button** in the resolver for a hint that says exactly what it does and what it throws away. The hints and the
column headings (*Ours · release-4*, *Theirs · bugfix*) name the real branches. **Use this result** and **Mark resolved**
stage the file but do not commit; you commit the merge afterwards with **Continue**.

**“Filled in from your earlier resolution.”** When Git already applied a resolution it remembered (see *Reuse my
resolution* above), the file has no conflict markers although it is still unresolved. The resolver then shows the result and
a **Use this result** button. Review it and press that. **Keep ours** or **Keep theirs** would throw the remembered
resolution away.

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
| A branch merged on GitHub/GitLab still looks unmerged, or a deleted branch is still listed | Treeline has not fetched since the change. It checks every 2 minutes unless you changed or turned it off in Settings → Background fetch; offline or missing sign-in stops it silently | Press **Fetch** now; if that fails, read the error (see *Signing in*) |
| *Uncommitted changes are in the way* | A checkout/merge would overwrite your edits | **Stash my changes…**, repeat, then pop |
| *The changes conflict and need your decision* | Merge/rebase conflict | Resolve in File status (section 10) |
| *Git doesn't know who you are* | No `user.name`/`user.email` | Run the two `git config` commands in section 1 |
| *A Git hook stopped the operation* | The repo's pre-commit/pre-push check failed | Read the technical details, fix, retry |
| *This branch isn't linked to a remote branch yet* | No upstream | Push with **Track this remote branch** |
| *Git is not installed (or not on PATH)* | The app could not start Git — programs opened from the Start menu or Finder do not always see your terminal's `PATH` | **Locate Git…** and pick `git.exe`, or install Git (section 1) |
| *A file or folder is in use or protected* | Another program (editor, antivirus, OneDrive) holds a file | Close it; move the repo out of OneDrive |
| *That folder has not been opened in Treeline* | Treeline only works on folders you chose or that are inside one | Use **Open repository…** (`Ctrl+O`) to pick it (section 2) |
| *This repository can run programs* | The folder's own settings or hooks can make Git run programs | Press **Open anyway** only if you trust where the folder came from (section 2) |
| *Your Git is older than the July 2025 security fixes* | An old Git has known clone and submodule bugs | Update Git (section 1) |
| *'…' is not a valid branch, tag, remote or commit name* | A name that starts with `-` would be read by Git as an option | Choose a different name |
| *Merged, but changes dropped* (Merge to many) | A branch kept its own version of a file | **Revert** that merge commit and merge again (section 10) |

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
| `Enter` / `Esc` or × (in a dialog) | Confirm / cancel |
| Double-click a file | Stage / unstage it |
| Double-click a branch | Check it out |
| Double-click a submodule | Open it in a tab beside its parent |

(On macOS use `Cmd` instead of `Ctrl`, except `Ctrl+Tab` for switching tabs. The app shows each shortcut with your platform's keys, for example ⌘K and ⌃Tab on a Mac. On a Mac, `F1` may need the `Fn` key, so `Cmd+?` is the easier way to open the guide.)

---

## 15. Tips for speed

* **Keep repositories out of OneDrive / Dropbox / network drives.** Sync tools lock and rescan thousands of small files
  inside `.git` and `node_modules`, which makes every Git tool slow. A plain folder such as `C:\dev` is much faster.
* The app refreshes by **watching the files** of the active tab — you don't need to press Refresh after editing in your
  editor.
* Very large histories load 400 commits at a time; keep scrolling to load more.
* Add `node_modules`, `dist` and build output to `.gitignore` so Git doesn't scan them.
* **Fix once, release everywhere.** Put the fix on one branch, then use **Merge to many** to bring it to every release
  branch in one go, with the conflicts listed before anything changes.

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
* **Hook** – a script in a repository that Git runs automatically at certain moments (before a commit, after a checkout …).
* **Rerere** (“reuse recorded resolution”) – a Git feature that remembers how you resolved a conflict and can apply the same
  fix when the identical conflict appears again.
