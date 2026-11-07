/** One-click fixes the UI knows how to perform. */
export type ErrorAction =
  | "pull"
  | "pull-rebase"
  | "stash"
  | "show-changes"
  | "remove-lock"
  | "choose-git"
  /** Last-resort fixes: shown only when the failed operation is known, and always confirmed first. */
  | "force-push"
  | "force-delete"
  | "force-checkout";

export interface Explained {
  /** Stable id of the matched rule ("unknown" when nothing matched). */
  id: string;
  title: string;
  /** What happened and why, in plain language. */
  why: string;
  /** Concrete things the user can do, most likely first. */
  fixes: string[];
  actions: ErrorAction[];
  /** For "remove-lock": the lock file git complained about. */
  lockPath?: string;
}

interface Rule {
  id: string;
  test: RegExp;
  build: (m: RegExpExecArray, raw: string) => Omit<Explained, "id">;
}

const RULES: Rule[] = [
  {
    id: "index-lock",
    test: /Unable to create '([^']*\.lock)': File exists/i,
    build: (m) => ({
      title: "Git is locked by another process",
      why: "Git creates a small lock file while it changes a repository. This one already exists, so either another Git program is working on the repository right now, or an earlier one crashed and left the file behind.",
      fixes: [
        "Wait a few seconds and try again — another tool (an IDE, another Git client, a terminal) may simply be busy.",
        "Close other programs that use this repository, then retry.",
        "If nothing else is running, the lock is stale: use “Remove lock file” below.",
      ],
      actions: ["remove-lock"],
      lockPath: m[1],
    }),
  },
  {
    id: "non-fast-forward",
    test: /non-fast-forward|\(fetch first\)|Updates were rejected because (the tip|the remote contains)|tip of your current branch is behind/i,
    build: () => ({
      title: "The remote has commits you don't have",
      why: "Someone (or you, from another machine) pushed new commits to this branch. Git refuses to overwrite them, so your push or fast-forward was rejected to protect that work.",
      fixes: [
        "Pull first to bring in the new commits, then push again (the Pull buttons below act on the branch you currently have checked out — check out the affected branch first if it is a different one). “Pull (rebase)” keeps history linear; “Pull” creates a merge commit.",
        "Only if you rewrote history on purpose (amend, rebase, reset) and want your version to REPLACE the remote one: use “Force push (with lease)…”. It asks you to confirm first, and it still refuses if someone pushed something new since your last fetch.",
      ],
      actions: ["pull", "pull-rebase", "force-push"],
    }),
  },
  {
    id: "auth",
    test: /Authentication failed|could not read (Username|Password)|terminal prompts disabled|Permission denied \(publickey|invalid credentials|HTTP Basic: Access denied|The requested URL returned error: 40[13]|fatal: unable to access .*: 40[13]/i,
    build: () => ({
      title: "Git could not sign in to the remote",
      why: "The server rejected your credentials, or none were available. This app never asks for passwords itself — it relies on your normal Git setup (credential manager or SSH key).",
      fixes: [
        "HTTPS remote: run any git command that needs the server once in a terminal (for example “git fetch”) and sign in; the credential manager will remember it. Check the token has not expired and has access to this repository.",
        "SSH remote: make sure your key is loaded (“ssh-add -l”) and that its public key is added to your account on the server. Test with “ssh -T git@<host>”.",
        "Check the remote URL is the right one (“git remote -v”).",
      ],
      actions: [],
    }),
  },
  {
    id: "repo-not-found",
    test: /Repository not found|does not appear to be a git repository|fatal: repository '.*' not found|remote: Not Found|returned error: 404/i,
    build: () => ({
      title: "The remote repository was not found",
      why: "The URL is wrong, the repository was renamed or deleted, or your account does not have access to it (many servers report “not found” instead of “forbidden”).",
      fixes: [
        "Double-check the URL for typos, including the user/organisation and repository name.",
        "Ask the repository owner to confirm you have access.",
        "Compare with “git remote -v” if this is an existing clone — the remote URL may be out of date.",
      ],
      actions: [],
    }),
  },
  {
    id: "network",
    test: /Could not resolve host|Failed to connect|Connection (timed out|refused|reset)|Network is unreachable|unable to access '.*': (Could not|Failed|Recv|SSL|Operation timed out)|Operation timed out|early EOF|RPC failed|Temporary failure in name resolution|Could not read from remote repository/i,
    build: () => ({
      title: "Could not reach the remote server",
      why: "The network request failed before Git could talk to the server — you may be offline, behind a VPN/proxy, or the server is down.",
      fixes: [
        "Check your internet connection and VPN, then try again.",
        "If you are behind a corporate proxy, make sure Git is configured for it (“git config --global http.proxy …”).",
        "If only this server fails, it may be temporarily down — retry in a few minutes.",
      ],
      actions: [],
    }),
  },
  {
    id: "overwritten-by-checkout",
    test: /Your local changes to the following files would be overwritten|would be overwritten by (checkout|merge)|Please commit your changes or stash them|cannot (pull|rebase|checkout|merge).*(unstaged|uncommitted) changes|You have unstaged changes|Please move or remove them before you (switch|merge)/i,
    build: () => ({
      title: "Uncommitted changes are in the way",
      why: "The operation would overwrite files you have modified but not committed, so Git stopped to avoid losing your work.",
      fixes: [
        "Stash your changes (they are saved and can be restored later), then repeat the operation and pop the stash afterwards.",
        "Or commit the changes first.",
        "Or discard them if you no longer need them: “Force checkout (discard my changes)…” — this cannot be undone, and you are asked to confirm.",
      ],
      actions: ["stash", "show-changes", "force-checkout"],
    }),
  },
  {
    id: "conflict",
    test: /CONFLICT \(|Automatic merge failed|fix conflicts and then commit|could not apply [0-9a-f]+|Resolve all conflicts manually|Merge conflict in/i,
    build: () => ({
      title: "The changes conflict and need your decision",
      why: "Both sides changed the same lines, so Git can't combine them automatically. The operation is paused, not failed — nothing is lost.",
      fixes: [
        "Open File status: conflicted files are listed at the top. Choose ours, theirs or both for each conflict, then “Mark resolved”.",
        "When every conflict is resolved, press Continue in the banner to finish. Press Abort to go back to how things were before.",
      ],
      actions: ["show-changes"],
    }),
  },
  {
    id: "unrelated-histories",
    test: /refusing to merge unrelated histories/i,
    build: () => ({
      title: "These branches share no common history",
      why: "The two sides were created independently (for example two separate “git init”s), so Git has no common ancestor to merge from.",
      fixes: [
        "Make sure you picked the right branch or remote.",
        "If you really want to join them, run “git merge --allow-unrelated-histories <branch>” in a terminal.",
      ],
      actions: [],
    }),
  },
  {
    id: "no-upstream",
    test: /has no upstream branch|There is no tracking information|no upstream configured|The current branch .* has no upstream/i,
    build: () => ({
      title: "This branch isn't linked to a remote branch yet",
      why: "Git doesn't know which remote branch to pull from or push to, because the local branch has never been pushed with tracking.",
      fixes: [
        "Use Push and tick “Track this remote branch” — this creates the remote branch and links them.",
        "To link an existing remote branch: “git branch --set-upstream-to=origin/<branch>”.",
      ],
      actions: [],
    }),
  },
  {
    id: "already-exists",
    test: /(branch|tag) named '?([^'\n]+)'? already exists|fatal: a (branch|tag) named .* already exists|already exists\.?$/im,
    build: () => ({
      title: "That name is already taken",
      why: "A branch or tag with this name already exists in the repository.",
      fixes: [
        "Choose a different name.",
        "Or delete/rename the existing one first (right-click it in the sidebar).",
      ],
      actions: [],
    }),
  },
  {
    id: "not-fully-merged",
    test: /is not fully merged/i,
    build: () => ({
      title: "That branch has commits that aren't merged anywhere",
      why: "Deleting it would make those commits hard to find again, so Git refused.",
      fixes: [
        "Merge the branch first if you still want its work.",
        "If you are sure you don't need those commits, use “Delete anyway…”. Commits only on that branch become unreachable (recoverable for a while via “git reflog”).",
      ],
      actions: ["force-delete"],
    }),
  },
  {
    id: "identity",
    test: /Please tell me who you are|unable to auto-detect email address|Author identity unknown|empty ident name/i,
    build: () => ({
      title: "Git doesn't know who you are",
      why: "Every commit records an author name and email, and neither is configured on this computer.",
      fixes: [
        "Run these once in a terminal, with your own details:  git config --global user.name \"Your Name\"  and  git config --global user.email \"you@example.com\"",
        "Then try the commit again.",
      ],
      actions: [],
    }),
  },
  {
    id: "hook",
    test: /pre-commit hook|commit-msg hook|pre-push hook|hook (failed|declined|returned)|husky/i,
    build: () => ({
      title: "A Git hook stopped the operation",
      why: "This repository runs a check (for example a linter or tests) before commits or pushes, and it reported a problem. The details are in the technical output below.",
      fixes: [
        "Read the technical details, fix what the hook reports (formatting, lint errors, failing tests), then try again.",
        "Run the same check in a terminal to see the full output.",
      ],
      actions: [],
    }),
  },
  {
    id: "nothing-to-commit",
    test: /nothing (added )?to commit|no changes added to commit/i,
    build: () => ({
      title: "There is nothing to commit",
      why: "No changes are staged, so a commit would be empty.",
      fixes: ["Stage the files you want (the + button next to each file), then commit."],
      actions: ["show-changes"],
    }),
  },
  {
    id: "empty-repo-ref",
    test: /src refspec .* does not match any|does not have any commits yet|unknown revision or path|bad revision|ambiguous argument 'HEAD'/i,
    build: () => ({
      title: "There is no commit to work with yet",
      why: "The branch or revision you asked for doesn't exist — usually because the repository has no commits yet.",
      fixes: ["Make your first commit, then try again.", "Check the branch or tag name for typos."],
      actions: [],
    }),
  },
  {
    id: "protocol-file",
    test: /transport 'file' not allowed/i,
    build: () => ({
      title: "Git blocked a local-folder submodule",
      why: "For security, recent Git versions refuse to clone submodules whose URL is a path on this computer.",
      fixes: [
        "If you trust the repository, run once in a terminal:  git -c protocol.file.allow=always submodule update --init",
        "Or change the submodule URL to a normal https:// or ssh:// address.",
      ],
      actions: [],
    }),
  },
  {
    id: "file-locked",
    test: /unable to (unlink|create|write)|cannot (unlink|remove)|Permission denied|being used by another process|Access is denied|failed to remove/i,
    build: () => ({
      title: "A file or folder is in use or protected",
      why: "Git couldn't change a file because another program has it open (common on Windows with editors, IDEs, antivirus or OneDrive) or you lack permission.",
      fixes: [
        "Close editors, terminals or build tools that may have files in the repository open, then retry.",
        "If the repository is inside OneDrive/Dropbox, pause syncing or move it to a plain folder such as C:\\dev.",
        "Check you have write permission for the folder.",
      ],
      actions: [],
    }),
  },
  {
    id: "not-a-repo",
    test: /not a git repository|This folder is not a Git repository/i,
    build: () => ({
      title: "This folder isn't a Git repository",
      why: "There is no .git folder here, so there is no history to show.",
      fixes: ["Open the repository's top folder, or clone it with Clone…", "To start a new repository, run “git init” in the folder first."],
      actions: [],
    }),
  },
  {
    id: "git-missing",
    test: /Could not run git|program not found|'git' is not recognized/i,
    build: () => ({
      title: "Git is not installed (or not on PATH)",
      why: "This app runs your system Git for every operation, and couldn't start it. Programs started from the Start menu or Finder don't always see the same PATH as your terminal.",
      fixes: [
        "If Git is already installed, choose its program (git.exe on Windows, usually in “C:\Program Files\Git\cmd”) with “Locate Git…”. The app remembers it.",
        "Otherwise install it. Windows: Git from git-scm.com. macOS: run “xcode-select --install” in Terminal, or “brew install git”. Linux: use your package manager.",
      ],
      actions: ["choose-git"],
    }),
  },
  {
    id: "push-rejected-generic",
    test: /failed to push some refs|\[remote rejected\]|pre-receive hook declined|protected branch/i,
    build: () => ({
      title: "The server rejected the push",
      why: "The remote accepted the connection but refused the update — often because the branch is protected, a server-side rule failed, or you lack write access.",
      fixes: [
        "Read the “remote:” lines in the technical details; the server usually says exactly why.",
        "If the branch is protected, push to a new branch and open a merge/pull request instead.",
        "Ask a repository admin if you need write access.",
      ],
      actions: [],
    }),
  },
];

/** Turns a raw git error into a title, explanation and fixes. Never returns null. */
export function explainError(raw: string): Explained {
  const text = raw ?? "";
  for (const rule of RULES) {
    const m = rule.test.exec(text);
    if (m) return { id: rule.id, ...rule.build(m, text) };
  }
  return {
    id: "unknown",
    title: "Something went wrong",
    why: "Git reported an error that this app doesn't have a specific explanation for. The exact message is in the technical details below.",
    fixes: [
      "Read the technical details — Git's messages usually say what to do.",
      "Press Refresh and try again; the state may have changed.",
      "Run the same operation in a terminal (“Terminal” button) to see the full output.",
    ],
    actions: [],
  };
}
