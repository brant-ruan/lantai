import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useT } from "../i18n";

/**
 * A small circled "?" that shows explanatory text in a floating bubble on
 * hover or keyboard focus. The bubble is portaled to <body> with fixed
 * positioning so scroll containers and modals never clip it.
 */
export function InfoTip({ children }: { children: ReactNode }) {
  const t = useT();
  const anchorRef = useRef<HTMLSpanElement>(null);
  const bubbleRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (!open || !anchorRef.current || !bubbleRef.current) return;
    const a = anchorRef.current.getBoundingClientRect();
    const b = bubbleRef.current.getBoundingClientRect();
    const margin = 8;
    let left = a.left + a.width / 2 - b.width / 2;
    left = Math.max(margin, Math.min(left, window.innerWidth - b.width - margin));
    // Prefer below the icon; flip above when there is no room.
    let top = a.bottom + 6;
    if (top + b.height > window.innerHeight - margin) top = a.top - b.height - 6;
    setPos({ top, left });
  }, [open]);

  const show = () => setOpen(true);
  const hide = () => {
    setOpen(false);
    setPos(null);
  };

  return (
    <>
      <span
        ref={anchorRef}
        className="info-tip"
        tabIndex={0}
        role="button"
        aria-label={t.infoTip.label}
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
      >
        ?
      </span>
      {open &&
        createPortal(
          <div
            ref={bubbleRef}
            role="tooltip"
            className="info-tip-bubble"
            style={
              pos
                ? { top: pos.top, left: pos.left }
                : { top: -9999, left: -9999 }
            }
          >
            {children}
          </div>,
          document.body,
        )}
    </>
  );
}
