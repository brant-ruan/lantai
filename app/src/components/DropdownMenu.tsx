import { useEffect, useRef, useState, type ReactNode } from "react";
import { MoreIcon } from "./icons";

interface DropdownMenuProps {
  label: ReactNode;
  align?: "left" | "right";
  direction?: "down" | "up";
  variant?: "secondary" | "ghost";
  size?: "sm" | "xs";
  hideChevron?: boolean;
  title?: string;
  /** Hover label (see `[data-tip]` in styles/controls.css); also used as the accessible name. */
  tip?: string;
  tipAlign?: "start" | "end";
  ariaLabel?: string;
  buttonClassName?: string;
  disabled?: boolean;
  children: (close: () => void) => ReactNode;
}

export function DropdownMenu({
  label,
  align = "right",
  direction = "down",
  variant = "secondary",
  size = "sm",
  hideChevron = false,
  title,
  tip,
  tipAlign,
  ariaLabel,
  buttonClassName,
  disabled = false,
  children,
}: DropdownMenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (ev: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(ev.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="dropdown">
      <button
        type="button"
        className={`btn btn-${variant} btn-${size}${
          buttonClassName ? ` ${buttonClassName}` : ""
        }`}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-label={ariaLabel ?? tip}
        title={title}
        data-tip={tip}
        data-tip-align={tipAlign}
        disabled={disabled}
      >
        {label}
        {!hideChevron && (
          <svg
            className="dropdown-chevron"
            width="10"
            height="10"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M4 6l4 4 4-4" />
          </svg>
        )}
      </button>
      {open && (
        <div className={`dropdown-menu align-${align} dir-${direction}`}>
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

/** The "more" menu at the right end of a toolbar: the same icon and place in every view. */
export function MoreMenu({
  tip,
  children,
}: {
  tip: string;
  children: (close: () => void) => ReactNode;
}) {
  return (
    <DropdownMenu
      label={<MoreIcon />}
      tip={tip}
      tipAlign="end"
      hideChevron
      variant="ghost"
      buttonClassName="toolbar-icon-btn"
    >
      {children}
    </DropdownMenu>
  );
}
