import { api, errorMessage, joinPath } from "./api";
import { useApp } from "./store";
import { useUi } from "./uiStore";
import { parseTrack } from "./lib/format";
import type { Commit, ErrorCtx, Submodule } from "./types";

/** All user-facing repository actions. Used by the toolbar, menus and the command palette. */
export function repoActions(tabId: string) {
  const st = () => useApp.getState();
  const path = () => st().tabs.find((t) => t.id === tabId)?.path ?? "";
  const data = () => st().data[tabId];
  const run = st().run.bind(null, tabId);
  const current = () => data()?.status?.branch ?? null;
  const localBranches = () => (data()?.refs ?? []).filter((r) => r.kind === "branch").map((r) => r.name);
  const remoteBranches = () => (data()?.refs ?? []).filter((r) => r.kind === "remote").map((r) => r.name);
  const confirm = (title: string, description: string, submitLabel: string, onSubmit: () => void | Promise<void>, danger = false) =>
    st().openDialog({ title, description, fields: [], submitLabel, danger, onSubmit });

  const a = {
    fetch: () =>
      run("Fetching…", () => api.fetch(tabId, path(), true), { success: "Fetched from all remotes" }),

    pull: (mode: "merge" | "rebase" | "ff-only" = "merge") =>
      run("Pulling…", () => api.pull(tabId, path(), mode), { quiet: false }),

    /** "origin/feature/x" → { remote: "origin", branch: "feature/x" } using the known remote names. */
    splitRemote: (full: string) => {
      const remotes = data()?.remotes ?? [];
      const remote = remotes.find((r) => full.startsWith(r + "/")) ?? full.split("/")[0];
      return { remote, branch: full.slice(remote.length + 1) };
    },

    /** Fetch just this branch from its remote (works for local branches with an upstream, and remote branches). */
    fetchBranch: (name: string, kind: "branch" | "remote") => {
      const ref = (data()?.refs ?? []).find((r) => r.kind === kind && r.name === name);
      const full = kind === "remote" ? name : ref?.upstream;
      if (!full) return st().notify("error", `${name} has no upstream branch to fetch from.`);
      const { remote, branch } = a.splitRemote(full);
      return run(`Fetching ${name}…`, () => api.fetchRef(tabId, path(), remote, branch), { success: `Fetched ${name}` });
    },

    /** Pull a local branch: merge/rebase when it is checked out, otherwise fast-forward it in place. */
    pullBranch: (name: string, mode: "merge" | "rebase" = "merge") => {
      const ref = (data()?.refs ?? []).find((r) => r.kind === "branch" && r.name === name);
      if (!ref?.upstream) return st().notify("error", `${name} has no upstream branch to pull from.`);
      if (ref.head) return a.pull(mode);
      const { remote, branch } = a.splitRemote(ref.upstream);
      return run(
        `Updating ${name}…`,
        () => api.fetchRef(tabId, path(), remote, `${branch}:${name}`),
        { success: `Fast-forwarded ${name}` },
      );
    },

    /** Push a local branch straight away, creating the upstream when it has none. */
    pushBranch: (name: string, force = false) => {
      const d = data();
      const ref = (d?.refs ?? []).find((r) => r.kind === "branch" && r.name === name);
      const remote = ref?.upstream ? a.splitRemote(ref.upstream).remote : d?.remotes[0];
      if (!remote) return st().notify("error", "This repository has no remotes yet.");
      const target = ref?.upstream ? a.splitRemote(ref.upstream).branch : name;
      return run(
        `Pushing ${name}…`,
        () => api.push(tabId, path(), { remote, refspec: `${name}:${target}`, setUpstream: !ref?.upstream, force }),
        {
          success: `Pushed ${name} to ${remote}`,
          ctx: { op: "push", remote, refspec: `${name}:${target}`, branch: name },
        },
      );
    },

    push: () => {
      const d = data();
      const remotes = d?.remotes ?? [];
      const branch = current();
      if (!branch) return st().notify("error", "Cannot push a detached HEAD. Create or check out a branch first.");
      if (remotes.length === 0) return st().notify("error", "This repository has no remotes yet.");
      const hasUpstream = !!d?.status?.upstream;
      const track = parseTrack((d?.refs ?? []).find((r) => r.kind === "branch" && r.name === branch)?.track ?? null);
      st().openDialog({
        title: "Push",
        description:
          track.ahead > 0 && track.behind > 0
            ? `This branch has diverged: you have ${track.ahead} commit${track.ahead > 1 ? "s" : ""} the server lacks and the server has ${track.behind} you lack. A normal push will be rejected. Pull first to combine them. Force push is only right if you rewrote history on purpose.`
            : track.behind > 0
              ? `The server has ${track.behind} commit${track.behind > 1 ? "s" : ""} you don't have. Pull first, then push.`
              : undefined,
        fields: [
          {
            key: "remote",
            label: "Remote",
            type: "select",
            value: d?.status?.upstream?.split("/")[0] ?? remotes[0],
            options: remotes.map((r) => ({ value: r, label: r })),
          },
          { key: "branch", label: "Branch", type: "text", value: branch },
          { key: "upstream", label: "Track this remote branch", type: "checkbox", value: !hasUpstream },
          { key: "tags", label: "Push all tags", type: "checkbox", value: false },
          { key: "force", label: "Force push (with lease)", type: "checkbox", value: false },
        ],
        submitLabel: "Push",
        onSubmit: async (v) => {
          void run("Pushing…", () =>
            api.push(tabId, path(), {
              remote: String(v.remote),
              refspec: String(v.branch),
              setUpstream: !!v.upstream,
              tags: !!v.tags,
              force: !!v.force,
            }),
            {
              success: `Pushed ${v.branch} to ${v.remote}`,
              ctx: { op: "push", remote: String(v.remote), refspec: String(v.branch), branch: String(v.branch) },
            },
          );
        },
      });
    },

    newBranch: (start?: string, defaultName = "") =>
      st().openDialog({
        title: "New branch",
        fields: [
          { key: "name", label: "Branch name", type: "text", value: defaultName, placeholder: "feature/my-change" },
          { key: "start", label: "Start from (blank = current commit)", type: "text", value: start ?? "" },
          { key: "checkout", label: "Check out the new branch", type: "checkbox", value: true },
        ],
        submitLabel: "Create branch",
        onSubmit: async (v) => {
          const name = String(v.name).trim();
          if (!name) throw new Error("Enter a branch name.");
          void run("Creating branch…", () => api.createBranch(path(), name, String(v.start).trim() || null, !!v.checkout), {
            success: `Created ${name}`,
          });
        },
      }),

    checkout: (name: string) =>
      run(`Checking out ${name}…`, () => api.checkout(path(), name), {
        success: `Switched to ${name}`,
        ctx: { op: "checkout", target: name },
      }),

    /** Checks out `target` and merges `source` into it, leaving any conflicts for the normal resolver. */
    async resolveMerge(target: string, source: string): Promise<void> {
      const ok = await run(`Checking out ${target}…`, () => api.checkout(path(), target), {
        ctx: { op: "checkout", target },
        quiet: true,
      });
      if (ok) await run(`Merging ${source} into ${target}…`, () => api.merge(path(), source, false, false), { success: `Merged ${source} into ${target}` });
    },

    /** Puts every checked-out submodule on `branch` where a branch of that name exists. */
    switchSubmodules: (branch: string) =>
      run("Switching submodules…", () => api.switchSubmodulesBranch(path(), branch), {
        quiet: false,
        success: undefined,
      }),

    /** Checks out `name` in this repository, then moves the submodules onto the same branch name. */
    async checkoutWithSubmodules(name: string): Promise<void> {
      const ok = await run(`Checking out ${name}…`, () => api.checkout(path(), name), {
        success: `Switched to ${name}`,
        ctx: { op: "checkout", target: name },
      });
      if (ok) await a.switchSubmodules(name);
    },

    // ---- last-resort "force" fixes: always explained and confirmed, never automatic ---------------
    forcePush: (ctx: Extract<ErrorCtx, { op: "push" }>) => {
      const ref = (data()?.refs ?? []).find((r) => r.kind === "branch" && r.name === ctx.branch);
      const behind = parseTrack(ref?.track ?? null).behind;
      const lost =
        behind > 0
          ? `${ctx.remote}/${ctx.branch} has about ${behind} commit${behind > 1 ? "s" : ""} that your branch does not contain (as of your last fetch). They will be removed from the server.`
          : `Any commits on ${ctx.remote}/${ctx.branch} that your branch does not contain will be removed from the server.`;
      confirm(
        `Force push ${ctx.branch}?`,
        `${lost} Anyone who already pulled them will need to repair their copy.\n\nThis uses "force with lease": git refuses if someone pushed something new since your last fetch, so it cannot silently destroy work you have not seen.\n\nDo this only if you rewrote history on purpose (amend, rebase, reset). If you are not sure, Pull first instead.`,
        "Force push",
        () =>
          void run("Force pushing…", () => api.push(tabId, path(), { remote: ctx.remote, refspec: ctx.refspec, force: true }), {
            success: `Force pushed ${ctx.branch}`,
          }),
        true,
      );
    },

    forceDelete: (name: string) =>
      confirm(
        `Delete ${name} even though it is not fully merged?`,
        "Commits that exist only on this branch will become unreachable. You can still recover them for a while with git reflog, but they will no longer appear in the history.\n\nChoose this only if you are sure you do not need that work.",
        "Delete anyway",
        () => void run("Deleting…", () => api.deleteBranch(path(), name, true), { success: `Deleted ${name}` }),
        true,
      ),

    forceCheckout: (target: string) =>
      confirm(
        `Switch to ${target} and DISCARD your local changes?`,
        `Your uncommitted changes to the files that conflict with "${target}" will be permanently thrown away. This cannot be undone.\n\nSafer: cancel, then use Stash my changes… so you can bring them back.`,
        "Discard changes and switch",
        () => void run(`Checking out ${target}…`, () => api.checkout(path(), target, true), { success: `Switched to ${target}` }),
        true,
      ),
    checkoutRemote: (name: string) =>
      run(`Checking out ${name}…`, () => api.checkoutRemote(path(), name), { success: `Checked out ${name}` }),

    merge: (rev?: string) => {
      const options = [...localBranches().filter((b) => b !== current()), ...remoteBranches()];
      st().openDialog({
        title: `Merge into ${current() ?? "HEAD"}`,
        fields: [
          rev
            ? { key: "rev", label: "Merge", type: "text", value: rev }
            : {
                key: "rev",
                label: "Branch to merge",
                type: "select",
                value: options[0] ?? "",
                options: options.map((o) => ({ value: o, label: o })),
              },
          { key: "noff", label: "Always create a merge commit (--no-ff)", type: "checkbox", value: false },
          { key: "squash", label: "Squash into a single change (not committed)", type: "checkbox", value: false },
        ],
        submitLabel: "Merge",
        onSubmit: async (v) => {
          if (!String(v.rev)) throw new Error("Choose something to merge.");
          void run("Merging…", () => api.merge(path(), String(v.rev), !!v.noff, !!v.squash), {
            success: `Merged ${v.rev}`,
          });
        },
      });
    },

    rebaseOnto: (name: string) =>
      confirm(
        `Rebase ${current()} onto ${name}?`,
        "This rewrites the commits of the current branch. Avoid it for branches others have pulled.",
        "Rebase",
        () => void run("Rebasing…", () => api.rebase(path(), name), { success: `Rebased onto ${name}` }),
      ),

    deleteBranch: (name: string) =>
      st().openDialog({
        title: `Delete branch ${name}?`,
        fields: [{ key: "force", label: "Delete even if it is not fully merged", type: "checkbox", value: false }],
        submitLabel: "Delete",
        danger: true,
        onSubmit: (v) =>
          void run("Deleting…", () => api.deleteBranch(path(), name, !!v.force), {
            success: `Deleted ${name}`,
            ctx: { op: "delete-branch", name },
          }),
      }),

    deleteRemoteBranch: (full: string) => {
      const [remote, ...rest] = full.split("/");
      confirm(
        `Delete ${full} from the server?`,
        "This removes the branch from the remote for everyone.",
        "Delete",
        () => void run("Deleting remote branch…", () => api.deleteRemoteBranch(tabId, path(), remote, rest.join("/")), { success: `Deleted ${full}` }),
        true,
      );
    },

    renameBranch: (name: string) =>
      st().openDialog({
        title: `Rename ${name}`,
        fields: [{ key: "name", label: "New name", type: "text", value: name }],
        submitLabel: "Rename",
        onSubmit: (v) => {
          const n = String(v.name).trim();
          if (!n) throw new Error("Enter a name.");
          void run("Renaming…", () => api.renameBranch(path(), name, n), { success: `Renamed to ${n}` });
        },
      }),

    newTag: (rev?: string) =>
      st().openDialog({
        title: "New tag",
        fields: [
          { key: "name", label: "Tag name", type: "text", placeholder: "v1.0.0" },
          { key: "rev", label: "At (blank = current commit)", type: "text", value: rev ?? "" },
          { key: "message", label: "Message (makes it an annotated tag)", type: "textarea" },
        ],
        submitLabel: "Create tag",
        onSubmit: (v) => {
          const n = String(v.name).trim();
          if (!n) throw new Error("Enter a tag name.");
          void run("Creating tag…", () => api.createTag(path(), n, String(v.rev).trim() || null, String(v.message).trim() || null), {
            success: `Created tag ${n}`,
          });
        },
      }),

    deleteTag: (name: string) =>
      confirm(`Delete tag ${name}?`, "Only the local tag is removed.", "Delete", () =>
        void run("Deleting tag…", () => api.deleteTag(path(), name), { success: `Deleted tag ${name}` }), true),

    pushTag: (name: string) => {
      const remote = data()?.remotes[0];
      if (!remote) return st().notify("error", "This repository has no remotes yet.");
      void run("Pushing tag…", () => api.push(tabId, path(), { remote, refspec: `refs/tags/${name}` }), { success: `Pushed tag ${name}` });
    },

    stash: () =>
      st().openDialog({
        title: "Stash changes",
        description: "Saves your local changes and restores a clean working tree.",
        fields: [
          { key: "message", label: "Message (optional)", type: "text" },
          { key: "untracked", label: "Include untracked files", type: "checkbox", value: true },
        ],
        submitLabel: "Stash",
        onSubmit: (v) =>
          void run("Stashing…", () => api.stashPush(path(), String(v.message).trim() || null, !!v.untracked), { success: "Changes stashed" }),
      }),

    stashAction: (ref: string, action: "apply" | "pop" | "drop") =>
      action === "drop"
        ? confirm(`Drop ${ref}?`, "This permanently deletes the stash.", "Drop", () => void run("Dropping stash…", () => api.stashAction(path(), ref, "drop")), true)
        : run(action === "pop" ? "Popping stash…" : "Applying stash…", () => api.stashAction(path(), ref, action)),

    cherryPick: (c: Commit) =>
      run("Cherry-picking…", () => api.cherryPick(path(), c.sha, c.parents.length > 1 ? 1 : null), { success: `Cherry-picked ${c.sha.slice(0, 7)}` }),

    revert: (c: Commit) =>
      run("Reverting…", () => api.revert(path(), c.sha, c.parents.length > 1 ? 1 : null), { success: `Reverted ${c.sha.slice(0, 7)}` }),

    reset: (sha: string, mode: "soft" | "mixed" | "hard") =>
      confirm(
        `${mode[0].toUpperCase() + mode.slice(1)} reset to ${sha.slice(0, 7)}?`,
        mode === "hard"
          ? "Hard reset discards ALL uncommitted changes and moves the branch. This cannot be undone."
          : mode === "soft"
            ? "Moves the branch and keeps all changes staged."
            : "Moves the branch and keeps all changes in the working tree, unstaged.",
        "Reset",
        () => void run("Resetting…", () => api.reset(path(), sha, mode), { success: `Reset to ${sha.slice(0, 7)}` }),
        mode === "hard",
      ),

    interactiveRebase: (c: Commit) =>
      useUi.setState({
        rebase: { tabId, base: c.parents[0] ?? null, title: `Interactive rebase from ${c.sha.slice(0, 7)}` },
      }),

    sequencer: (state: string, action: "continue" | "skip" | "abort") =>
      run(`${action[0].toUpperCase() + action.slice(1)}…`, () => api.sequencer(path(), state, action), {
        success: action === "abort" ? `Aborted ${state}` : undefined,
      }),

    updateSubmodules: () =>
      run("Updating submodules…", () => api.updateSubmodules(path(), false), { success: "Submodules updated" }),

    /** Opens a submodule in its own tab, initialising it first if it was never checked out. */
    async openSubmodule(m: Submodule): Promise<void> {
      try {
        if (m.state === "uninitialized") {
          const res = await api.initSubmodule(path(), m.path);
          if (!res.ok) throw new Error(res.output);
          void st().reload(tabId);
        }
        await st().openRepo(joinPath(path(), m.path), tabId); // opens next to this (parent) tab
      } catch (e) {
        st().notify("error", errorMessage(e));
      }
    },

    reveal: () => void api.reveal(path()).catch((e) => st().notify("error", errorMessage(e))),
    terminal: () => void api.terminal(path()).catch((e) => st().notify("error", errorMessage(e))),
    copy: (text: string) => void navigator.clipboard?.writeText(text).then(() => st().notify("success", "Copied to clipboard")),
  };
  return a;
}
