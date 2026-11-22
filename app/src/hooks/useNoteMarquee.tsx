// Selecting human notes by dragging a box over them, as in a file manager. The drag starts on a
// note's row or on empty space of the report (never on report text, which stays selectable for
// copying); Shift or Cmd adds to the selection. Backspace or Delete removes the selected notes,
// Escape clears the selection, and Cmd+Z outside a text field undoes the last deletion.
import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { HumanJournal } from "./useHumanJournal";
import { setPointerDrag } from "./pointerDrag";

/** Containers whose empty space (the element itself, not its text) can start a drag. */
const BLANK =
  ".report-content, .report-document, .report-sections, .markdown-body, .report-section-card, " +
  ".report-section-body, .human-day-notes, .human-section-notes, .human-blocks, " +
  ".human-children, .human-annotations, .human-block";
const INTERACTIVE =
  "textarea, input, select, button, a, [contenteditable], [role='separator'], .dropdown";
/** Movement before a press becomes a drag, so that a click still opens a note. */
const THRESHOLD = 5;
/** Distance from the top or bottom edge at which the report scrolls while dragging. */
const EDGE = 40;

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

function editing(): boolean {
  const el = document.activeElement as HTMLElement | null;
  return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
}

export function useNoteMarquee(journal: HumanJournal, enabled: boolean): React.ReactNode {
  const [box, setBox] = useState<Box | null>(null);
  const ref = useRef(journal);
  ref.current = journal;

  useEffect(() => {
    if (!enabled) return;

    const onDown = (e: MouseEvent) => {
      if (e.button !== 0) return;
      const target = e.target as HTMLElement;
      const additive = e.shiftKey || e.metaKey;
      if (!additive && ref.current.selected.size > 0) ref.current.select(new Set());
      const area = target.closest<HTMLElement>(".report-content");
      if (!area || target.closest(INTERACTIVE)) return;
      if (!target.closest("[data-note-id]") && !target.matches(BLANK)) return;
      // Keeping the press from selecting text also keeps focus where it was, so a note being
      // edited would stay open; a click beside it ends the edit, as anywhere else on the page.
      e.preventDefault();
      if (editing()) (document.activeElement as HTMLElement).blur();

      const base = additive ? ref.current.selected : new Set<number>();
      const start = { x: e.clientX, y: e.clientY + area.scrollTop };
      let pointer = { x: e.clientX, y: e.clientY };
      let active = false;
      let frame = 0;

      const update = () => {
        const y = pointer.y + area.scrollTop;
        const left = Math.min(start.x, pointer.x);
        const right = Math.max(start.x, pointer.x);
        const top = Math.min(start.y, y) - area.scrollTop;
        const bottom = Math.max(start.y, y) - area.scrollTop;
        setBox({ left, top, width: right - left, height: bottom - top });
        const ids = new Set(base);
        area.querySelectorAll<HTMLElement>("[data-note-id]").forEach((row) => {
          const r = row.getBoundingClientRect();
          if (r.right >= left && r.left <= right && r.bottom >= top && r.top <= bottom) {
            ids.add(Number(row.dataset.noteId));
          }
        });
        ref.current.select(ids);
      };

      // Keeps scrolling while the pointer rests near an edge.
      const tick = () => {
        const r = area.getBoundingClientRect();
        const dy =
          pointer.y < r.top + EDGE
            ? -Math.ceil((r.top + EDGE - pointer.y) / 4)
            : pointer.y > r.bottom - EDGE
              ? Math.ceil((pointer.y - (r.bottom - EDGE)) / 4)
              : 0;
        if (dy !== 0) {
          area.scrollTop += dy;
          update();
        }
        frame = requestAnimationFrame(tick);
      };

      const onMove = (ev: MouseEvent) => {
        pointer = { x: ev.clientX, y: ev.clientY };
        if (!active) {
          if (Math.hypot(pointer.x - e.clientX, pointer.y - e.clientY) < THRESHOLD) return;
          active = true;
          setPointerDrag(true);
          window.getSelection()?.removeAllRanges();
          frame = requestAnimationFrame(tick);
        }
        update();
      };

      const swallowClick = (ev: MouseEvent) => {
        ev.stopPropagation();
        ev.preventDefault();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        if (!active) return;
        cancelAnimationFrame(frame);
        setPointerDrag(false);
        setBox(null);
        // The click that ends a drag must not open the note under the pointer.
        window.addEventListener("click", swallowClick, { capture: true, once: true });
        setTimeout(() => window.removeEventListener("click", swallowClick, { capture: true }));
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    };

    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing || editing()) return;
      const j = ref.current;
      if ((e.key === "Backspace" || e.key === "Delete") && j.selected.size > 0) {
        e.preventDefault();
        void j.remove([...j.selected]);
      } else if (e.key === "Escape" && j.selected.size > 0) {
        j.select(new Set());
      } else if (
        e.key.toLowerCase() === "z" &&
        (e.metaKey || e.ctrlKey) &&
        !e.shiftKey &&
        !e.altKey &&
        j.undo()
      ) {
        e.preventDefault();
      }
    };

    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [enabled]);

  return box && createPortal(<div className="note-marquee" style={box} />, document.body);
}
