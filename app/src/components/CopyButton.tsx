import { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";

/**
 * Icon-only "copy as Markdown" button. It stays hidden until the surrounding
 * `.report-panel` or drawer `.turn-card` is hovered and shows a check mark briefly
 * after a successful copy. `text` runs on click (the clipboard write must stay in
 * the click), so the copy is always what is on screen at that moment.
 */
export function CopyButton({ text }: { text: () => string }) {
  const t = useT().copy;
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text());
      setState("copied");
    } catch {
      setState("failed");
    }
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setState("idle"), 1500);
  };

  const label =
    state === "copied"
      ? t.copied
      : state === "failed"
        ? t.failed
        : t.asMarkdown;

  return (
      <button
        type="button"
        className={`report-copy-btn ${state}`}
        onClick={() => void copy()}
        title={label}
        aria-label={label}
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          {state === "copied" ? (
            <polyline points="4 12.5 9.5 18 20 6" />
          ) : state === "failed" ? (
            <path d="M6 6l12 12M18 6L6 18" />
          ) : (
            <>
              <rect x="9" y="9" width="11" height="11" rx="2" />
              <path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1" />
            </>
          )}
        </svg>
      </button>
  );
}
