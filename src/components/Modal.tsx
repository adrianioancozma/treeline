import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { closeModal, isTopModal, modalKeyAction, nextFocusIndex, openModal } from "../lib/modal";
import { Icon } from "./Icon";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * The frame every dialog uses: an X in the corner, Escape and a click on the backdrop close it (not while `busy`),
 * focus moves into the dialog, Tab stays inside it, and focus goes back to where it was when it closes.
 */
export function Modal({
  title,
  label,
  onClose,
  busy = false,
  className = "",
  headerExtra,
  onKeyDown,
  children,
}: {
  title: ReactNode;
  /** Accessible name when `title` is not plain text. */
  label?: string;
  onClose: () => void;
  busy?: boolean;
  className?: string;
  headerExtra?: ReactNode;
  onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void;
  children: ReactNode;
}) {
  const id = useId();
  const ref = useRef<HTMLDivElement>(null);
  const state = useRef({ busy, onClose });
  state.current = { busy, onClose };
  // Read during the first render: by the time effects run, an autoFocus field inside the dialog already has focus.
  const [previous] = useState(() => (document.activeElement instanceof HTMLElement ? document.activeElement : null));

  useEffect(() => {
    openModal(id);
    const box = ref.current;
    // A field with autoFocus already has focus; otherwise take the first control that is not the X, else the dialog itself.
    if (box && !box.contains(document.activeElement)) {
      const first = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].find((el) => !el.classList.contains("modal-close"));
      (first ?? box).focus();
    }
    const onKey = (e: globalThis.KeyboardEvent) => {
      const action = modalKeyAction(e.key, { top: isTopModal(id), busy: state.current.busy });
      if (action === "close") {
        e.preventDefault();
        state.current.onClose();
      } else if (action === "trap" && box) {
        const items = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
        const next = nextFocusIndex(items.length, items.indexOf(document.activeElement as HTMLElement), e.shiftKey);
        e.preventDefault();
        (next >= 0 ? items[next] : box).focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      closeModal(id);
      if (previous?.isConnected) previous.focus();
    };
  }, [id, previous]);

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div
        ref={ref}
        className={`modal ${className}`.trim()}
        role="dialog"
        aria-modal="true"
        aria-labelledby={label ? undefined : `${id}-title`}
        aria-label={label}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <div className="modal-head">
          <h2 id={`${id}-title`} className="ellipsis">
            {title}
          </h2>
          {headerExtra}
          <button className="icon-btn modal-close" aria-label="Close" title="Close (Esc)" onClick={onClose} disabled={busy}>
            <Icon name="x" size={14} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
