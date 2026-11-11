# Testing Treeline on macOS and Linux

**Status:** the app has been built and tested on **Windows 10** and on **Apple Silicon Macs**. It has **never been run
on Linux or on an Intel Mac**. This checklist is how you can help change that. It takes about 15–20 minutes. Thank you!

You need a Mac (macOS 12 or newer) or a Linux machine with a desktop (Debian/Ubuntu is what the scripts target), and
`git` installed. Two small test repositories are enough: any repository you can clone, plus a scratch one you make.

## 0. Build or install

| System | How |
| --- | --- |
| macOS | See "macOS" in the [README](../README.md#macos-and-linux). Build with `pnpm tauri build --target universal-apple-darwin`, or run `pnpm tauri dev` to try it without building an installer. |
| Linux | Run `bash scripts/setup-linux.sh` once, then `bash scripts/build-linux.sh` (it runs all tests first). To try without packaging: `pnpm tauri dev`. |

**Please report if the build itself fails**, with the full error text and the output of `rustc --version`,
`node --version` and `pnpm --version`. Also tell us if `cargo test --lib` (in `src-tauri`) fails, and which tests.

## 1. Smoke test

Tick each line as pass (✅), fail (❌) or skipped (➖). Note anything odd even if it "works".

**Start-up and Git**
1. ☐ The app starts and shows the home screen. Note any warning from the OS (Gatekeeper, missing library …).
2. ☐ **macOS only:** launch it by **double-clicking the app / from the Dock** (not from a terminal) and open a repository.
   This checks that Homebrew's `git` is found when the app has a minimal `PATH`. If it says Git is missing, that is a bug.
3. ☐ Open a repository (`Ctrl/Cmd+O`). Branch name, status and history appear.
4. ☐ Quit and restart: your tabs come back.

**Daily workflow**
5. ☐ Edit a file in your editor — it appears in *File status* **by itself**, without pressing Refresh (file watcher).
   Also, in a **test** repository with a remote, push a commit from a second clone: within about 2 minutes (the default; see Settings → Background fetch) it appears in *History* without pressing Fetch (background fetch).
6. ☐ Stage a file, stage a single **hunk**, stage **selected lines**; commit; amend the commit message.
7. ☐ *History*: scrolls smoothly; click a commit and see its diff; click a branch in the sidebar and see it highlighted.
8. ☐ Create a branch, check it out, delete it.
9. ☐ Clone a repository over **HTTPS** and one over **SSH** (if you can). A credential/sign-in prompt from your system's
   keychain or credential helper should appear when needed. Progress bar updates while cloning.
10. ☐ Fetch, pull and push (a scratch remote is fine — a bare repo on disk works).

**The harder features**
11. ☐ Open a **submodule** (double-click it in the sidebar): it opens in a tab right beside its parent.
12. ☐ Cause a **merge conflict** (two branches editing the same line), resolve it in the app, mark resolved, Continue.
13. ☐ **Interactive rebase** (right-click a commit): reword one commit and squash another.
14. ☐ **Stash** with "include untracked files": the working tree becomes clean; Pop restores it.
15. ☐ Create a stale lock (`touch .git/index.lock` in the repo) and press *Stage all*: an explanation appears, and
    **Remove lock file** works.

**Platform behaviour**
16. ☐ Shortcuts: `Ctrl/Cmd+K` palette, `+T` new tab, `+W` closes the **tab** (on macOS it must **not** close the whole window),
    `+O` open, `+R` refresh, `Ctrl+Tab` next tab, `F1` or `Cmd/Ctrl+?` opens the guide.
17. ☐ **macOS only:** `Cmd+C`, `Cmd+V`, `Cmd+A`, `Cmd+Z` work in the commit message box and text fields; the menu bar looks sensible;
    `Cmd+Q` quits.
18. ☐ The **Terminal** button opens a terminal in the repository folder; **Explorer** opens the folder in your file manager.
19. ☐ The built-in guide (`F1`) opens in its own window and works with the network off.
20. ☐ The theme follows your system's light/dark setting, and the ☀ button switches it.
21. ☐ Looks right: fonts, spacing, icons, scrolling, on a high-DPI/Retina screen; the window resizes down to its minimum without breakage.
22. ☐ **Linux only:** the file watcher still reacts on a large repository (if you see "too many open files", note it); under Wayland and X11 if you
    can try both.

## 2. Report what you found

Please send it to whoever maintains this repository (open an issue, or send the text any way they prefer). Copy this template:

```
**Result:** works / partly works / does not work

**System:** macOS 14.5 (Apple Silicon M2)  |  Ubuntu 24.04 (GNOME, Wayland)  |  …
**git --version:**
**App version / commit:**
**How it was built or installed:**
**How it was launched:** Finder / Dock / terminal / AppImage / …

**Checklist:** (paste the list with ✅ ❌ ➖)

**Problems** (one per item):
- What I did:
- What I expected:
- What happened:
- Screenshot or error text ("Copy details" in an error panel, or terminal output):
```

Even a short "everything in 1–15 worked on macOS 14" is a very useful report.
