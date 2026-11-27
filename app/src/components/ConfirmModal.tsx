// The shell of a confirmation dialog: title, body and buttons above everything else, including
// Settings. The app's WebView shows no `window.confirm`, so every confirmation goes through this.

import { useEffect, useId, type ReactNode } from "react";

export function ConfirmModal({
  title,
  children,
  actions,
  onClose,
}: {
  title: string;
  children?: ReactNode;
  actions: ReactNode;
  onClose: () => void;
}) {
  const titleId = useId();

  // Escape closes this dialog only, not a modal underneath (Settings): listen first and stop it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopImmediatePropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="modal-backdrop confirm-backdrop" onClick={onClose}>
      <div
        className="confirm-modal"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="confirm-modal-body">
          <h2 id={titleId}>{title}</h2>
          {children}
        </div>
        <div className="export-modal-foot">{actions}</div>
      </div>
    </div>
  );
}
