import { useEffect, useRef, useState } from "react";
import { useApp } from "../store";
import type { DialogSpec } from "../store";
import { closeMenu, useUi } from "../uiStore";
import { errorMessage } from "../api";
import { Icon } from "./Icon";
import { ErrorBox } from "./ErrorBox";

export function ContextMenuHost() {
  const menu = useUi((s) => s.menu);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });

  useEffect(() => {
    if (!menu) return;
    const el = ref.current;
    const w = el?.offsetWidth ?? 220;
    const h = el?.offsetHeight ?? 200;
    setPos({
      x: Math.min(menu.x, window.innerWidth - w - 8),
      y: Math.min(menu.y, window.innerHeight - h - 8),
    });
    const close = () => closeMenu();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("wheel", close, { passive: true });
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("wheel", close);
    };
  }, [menu]);

  if (!menu) return null;
  return (
    <div
      ref={ref}
      className="ctx-menu"
      style={{ left: pos.x, top: pos.y }}
      role="menu"
      onMouseDown={(e) => e.stopPropagation()}
    >
      {menu.items.map((it, i) =>
        it.separator ? (
          <hr key={i} />
        ) : (
          <button
            key={i}
            role="menuitem"
            className={it.danger ? "danger" : ""}
            disabled={it.disabled}
            onClick={() => {
              closeMenu();
              it.onClick?.();
            }}
          >
            <span>{it.label}</span>
            {it.hint && <span className="hint-key">{it.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}

export function Toasts() {
  const { toasts, dismiss } = useApp();
  if (toasts.length === 0) return null;
  return (
    <div className="toasts" aria-live="polite">
      {toasts.length > 1 && (
        <button className="btn tiny dismiss-all" onClick={() => toasts.forEach((t) => dismiss(t.id))}>
          Dismiss all
        </button>
      )}
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`} role={t.kind === "error" ? "alert" : "status"}>
          {t.kind === "error" ? <ErrorBox text={t.text} compact ctx={t.ctx} onDone={() => dismiss(t.id)} /> : <pre>{t.text}</pre>}
          <button className="icon-btn" aria-label="Dismiss" onClick={() => dismiss(t.id)}>
            <Icon name="x" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}

export function FormDialogHost() {
  const dialog = useApp((s) => s.dialog);
  return dialog ? <FormDialog key={dialog.title} spec={dialog} /> : null;
}

function FormDialog({ spec }: { spec: DialogSpec }) {
  const close = useApp((s) => s.closeDialog);
  const [values, setValues] = useState<Record<string, string | boolean>>(() =>
    Object.fromEntries(spec.fields.map((f) => [f.key, f.value ?? (f.type === "checkbox" ? false : "")])),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await spec.onSubmit(values);
      close();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && close()}>
      <div
        className="modal"
        role="dialog"
        aria-label={spec.title}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape" && !busy) close();
          if (e.key === "Enter" && !(e.target instanceof HTMLTextAreaElement) && !busy) void submit();
        }}
      >
        <h2>{spec.title}</h2>
        {spec.description && <p className="muted small">{spec.description}</p>}
        {spec.fields.map((f, i) => {
          if (f.showIf && !values[f.showIf]) return null;
          const set = (v: string | boolean) => setValues((s) => ({ ...s, [f.key]: v }));
          if (f.type === "checkbox")
            return (
              <label key={f.key} className="check">
                <input type="checkbox" checked={!!values[f.key]} onChange={(e) => set(e.target.checked)} />
                {f.label}
              </label>
            );
          return (
            <label key={f.key}>
              {f.label}
              {f.type === "select" ? (
                <select value={String(values[f.key])} onChange={(e) => set(e.target.value)}>
                  {f.options?.map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </select>
              ) : f.type === "textarea" ? (
                <textarea
                  rows={3}
                  value={String(values[f.key])}
                  placeholder={f.placeholder}
                  onChange={(e) => set(e.target.value)}
                />
              ) : (
                <input
                  autoFocus={i === 0}
                  value={String(values[f.key])}
                  placeholder={f.placeholder}
                  onChange={(e) => set(e.target.value)}
                />
              )}
            </label>
          );
        })}
        {error && <ErrorBox text={error} compact />}
        <div className="modal-actions">
          <button className="btn" onClick={close} disabled={busy}>
            Cancel
          </button>
          <button className={`btn ${spec.danger ? "danger" : "primary"}`} onClick={submit} disabled={busy}>
            {spec.submitLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
