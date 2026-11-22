// The triangle in front of a note block with children, in place of its bullet: the same
// control as in the report outline, tinted for human notes.
import type React from "react";
import { useT } from "../../i18n";

export function NoteToggle({
  expanded,
  onToggle,
  onMouseDown,
}: {
  expanded: boolean;
  onToggle: () => void;
  /** Starts a drag of the block (see `useNoteDrag`). */
  onMouseDown?: (e: React.MouseEvent) => void;
}) {
  const t = useT().human;
  return (
    <button
      type="button"
      className={`outline-toggle human-toggle${expanded ? " expanded" : ""}`}
      aria-expanded={expanded}
      title={expanded ? t.collapse : t.expand}
      onClick={onToggle}
      onMouseDown={onMouseDown}
    >
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M5.25 3.25L11.25 8L5.25 12.75V3.25Z" fill="currentColor" />
      </svg>
    </button>
  );
}
