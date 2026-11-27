// Single button that flips a report outline between the L2 overview and fully expanded details.

import { useT } from "../../i18n";

interface OutlineToggleProps {
  expanded: boolean;
  onToggle: () => void;
}

export function OutlineToggle({ expanded, onToggle }: OutlineToggleProps) {
  const t = useT().outline;
  const label = expanded ? t.collapse : t.expand;
  return (
    <button
      type="button"
      className="btn btn-ghost btn-xs outline-all-toggle"
      onClick={onToggle}
      title={expanded ? t.collapseTitle : t.expandTitle}
      aria-pressed={expanded}
    >
      <svg
        width="12"
        height="12"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        {expanded ? (
          <path d="M4 10l4-4 4 4" />
        ) : (
          <path d="M4 6l4 4 4-4" />
        )}
      </svg>
      {label}
    </button>
  );
}
