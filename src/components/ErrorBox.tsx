import { useMemo, useState } from "react";
import { repoActions } from "../actions";
import { api, errorMessage } from "../api";
import { open } from "@tauri-apps/plugin-dialog";
import { explainError, type ErrorAction } from "../lib/explain";
import { GUIDE, openGuide } from "../guide";
import { useApp } from "../store";
import type { ErrorCtx } from "../types";

const ACTION_LABEL: Record<ErrorAction, string> = {
  pull: "Pull",
  "pull-rebase": "Pull (rebase)",
  stash: "Stash my changes…",
  "show-changes": "Show changes",
  "remove-lock": "Remove lock file",
  "choose-git": "Locate Git…",
  reconnect: "Reconnect",
  "force-push": "Force push (with lease)…",
  "force-delete": "Delete anyway…",
  "force-checkout": "Force checkout (discard my changes)…",
};

const isForce = (a: ErrorAction) => a.startsWith("force-");

function forceAvailable(a: ErrorAction, ctx?: ErrorCtx): boolean {
  return (
    (a === "force-push" && ctx?.op === "push") ||
    (a === "force-delete" && ctx?.op === "delete-branch") ||
    (a === "force-checkout" && ctx?.op === "checkout")
  );
}

/**
 * Shows an error the way a helpful colleague would: what happened, why, how to fix it, one-click
 * fixes where possible, and the raw git output tucked under "Technical details".
 */
export function ErrorBox({
  text,
  compact = false,
  onDone,
  ctx,
  onRetry,
}: {
  text: string;
  compact?: boolean;
  onDone?: () => void;
  /** The operation that failed; enables the matching force fix. */
  ctx?: ErrorCtx;
  /** Called after Reconnect succeeds, for a host that shows something other than a tab (the remote browser). */
  onRetry?: () => void;
}) {
  const ex = useMemo(() => explainError(text), [text]);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  /** Lets the user pick the git executable; it is verified and remembered by the backend. */
  const chooseGit = async () => {
    const s = useApp.getState();
    setBusy(true);
    try {
      const file = await open({
        multiple: false,
        directory: false,
        title: "Locate the Git program (git.exe)",
        filters: [{ name: "Git", extensions: ["exe"] }, { name: "All files", extensions: ["*"] }],
      });
      if (typeof file !== "string") return;
      const info = await api.setGitPath(file);
      s.notify("success", `Using ${info.version ?? "Git"} at ${info.path}. You can try again now.`);
      onDone?.();
    } catch (e) {
      s.notify("error", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const run = async (action: ErrorAction) => {
    const s = useApp.getState();
    if (action === "choose-git") return chooseGit();
    if (action === "reconnect") {
      // The computer comes from the error itself: the toast may be about a tab that is no longer the active one.
      if (!ex.host) return;
      setBusy(true);
      try {
        await s.reconnectHost(ex.host);
        onDone?.();
        onRetry?.();
      } catch (e) {
        s.notify("error", errorMessage(e));
      } finally {
        setBusy(false);
      }
      return;
    }
    const tab = s.tabs.find((t) => t.id === s.activeId);
    if (!tab?.path) return;
    const act = repoActions(tab.id);
    // The error has been acted on; let the host (e.g. the toast) dismiss it.
    if (action !== "remove-lock" && !isForce(action)) onDone?.();
    switch (action) {
      case "pull":
        return void act.pull("merge");
      case "pull-rebase":
        return void act.pull("rebase");
      case "stash":
        return act.stash();
      case "show-changes":
        return s.setUi(tab.id, { view: "files" });
      case "force-push":
        return ctx?.op === "push" ? act.forcePush(ctx) : undefined;
      case "force-delete":
        return ctx?.op === "delete-branch" ? act.forceDelete(ctx.name) : undefined;
      case "force-checkout":
        return ctx?.op === "checkout" ? act.forceCheckout(ctx.target) : undefined;
      case "remove-lock": {
        if (!ex.lockPath) return;
        setBusy(true);
        try {
          await api.removeLockFile(tab.path, ex.lockPath);
          s.notify("success", "Lock file removed. You can try the operation again.");
          onDone?.();
          void s.reload(tab.id);
        } catch (e) {
          s.notify("error", errorMessage(e));
        } finally {
          setBusy(false);
        }
      }
    }
  };

  const copy = () => {
    void navigator.clipboard?.writeText(`${ex.title}\n\n${text}`).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <div className={`err-box${compact ? " compact" : ""}`} role="alert">
      <strong className="err-title">{ex.title}</strong>
      <p className="err-why">{ex.why}</p>
      {ex.fixes.length > 0 && (
        <>
          <h4>How to fix it</h4>
          <ol>
            {ex.fixes.map((f, i) => (
              <li key={i}>{f}</li>
            ))}
          </ol>
        </>
      )}
      <div className="err-actions">
        {ex.actions
          .filter((a) => !isForce(a) || forceAvailable(a, ctx))
          .map((a) => (
            <button
              key={a}
              className={isForce(a) ? "btn tiny danger-outline" : "btn tiny primary"}
              disabled={busy}
              title={isForce(a) ? "Risky last resort: you will be asked to confirm" : undefined}
              onClick={() => void run(a)}
            >
              {ACTION_LABEL[a]}
            </button>
          ))}
        <button className="btn tiny" onClick={() => void openGuide(GUIDE.errors)} title="Open the guide page on fixing errors">
          Learn more
        </button>
        <button className="btn tiny" onClick={copy}>
          {copied ? "Copied ✓" : "Copy details"}
        </button>
      </div>
      <details className="err-tech">
        <summary>Technical details</summary>
        <pre>{text.trim() || "(no output)"}</pre>
      </details>
    </div>
  );
}
