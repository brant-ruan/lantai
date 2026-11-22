// Multi-selection of session slices in a scrolling list. Dragging a rectangle over rows
// selects them (holding Cmd/Ctrl/Shift adds to the selection), Cmd/Ctrl-click toggles a
// row, Shift-click selects a range, and Escape clears. A non-empty selection is what puts
// the list into batch mode; plain clicks keep opening the slice.
//
// Rows mark themselves with `data-slice-id`; the scroll element gets `scrollRef` and
// `onMouseDown`, and renders `marquee` (content coordinates) as an overlay.

import React, { useCallback, useEffect, useRef, useState } from "react";
import { isTextInput, modalOpen } from "../dom";

/** Pointer travel before a press becomes a rectangle drag instead of a click. */
const DRAG_THRESHOLD = 4;
/** Distance from the top or bottom edge where a drag scrolls the list, and the step. */
const EDGE = 36;
const EDGE_STEP = 14;
/** Presses on these keep their own behaviour and never start a rectangle. */
const NO_DRAG = "input, select, textarea, a, form, .slice-controls, .batch-bar, .banner";

export interface MarqueeRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function useSliceSelection(orderedIds: number[], resetKey: unknown) {
  const [selected, setSelectedState] = useState<number[]>([]);
  const [marquee, setMarquee] = useState<MarqueeRect | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const anchorRef = useRef<number | null>(null);
  const orderRef = useRef(orderedIds);
  orderRef.current = orderedIds;
  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  /** Replace the selection, kept in list order and limited to listed rows. */
  const setSelected = useCallback((ids: Iterable<number>) => {
    const set = new Set(ids);
    setSelectedState(orderRef.current.filter((id) => set.has(id)));
  }, []);

  const clear = useCallback(() => {
    setSelectedState([]);
    anchorRef.current = null;
  }, []);

  // A different day, week or project: slice ids no longer mean the same rows.
  useEffect(clear, [resetKey, clear]);

  // Rows that left the list (filter change, refresh) drop out of the selection.
  useEffect(() => {
    const listed = new Set(orderedIds);
    setSelectedState((prev) => {
      const next = prev.filter((id) => listed.has(id));
      return next.length === prev.length ? prev : next;
    });
  }, [orderedIds]);

  const active = selected.length > 0;
  useEffect(() => {
    if (!active) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || isTextInput(e.target) || modalOpen()) return;
      clear();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [active, clear]);

  const toggle = useCallback(
    (id: number) => {
      const next = new Set(selectedRef.current);
      if (!next.delete(id)) next.add(id);
      setSelected(next);
      anchorRef.current = id;
    },
    [setSelected],
  );

  const rangeTo = useCallback(
    (id: number) => {
      const order = orderRef.current;
      const anchor = anchorRef.current ?? id;
      const i1 = order.indexOf(anchor);
      const i2 = order.indexOf(id);
      if (i1 === -1 || i2 === -1) {
        toggle(id);
        return;
      }
      const next = new Set(selectedRef.current);
      for (const x of order.slice(Math.min(i1, i2), Math.max(i1, i2) + 1)) next.add(x);
      setSelected(next);
      anchorRef.current = anchor;
    },
    [setSelected, toggle],
  );

  /** Handle a modifier click on a row; returns false for a plain click. */
  const onRowClick = useCallback(
    (e: React.MouseEvent, id: number): boolean => {
      if (e.metaKey || e.ctrlKey) {
        toggle(id);
      } else if (e.shiftKey) {
        rangeTo(id);
      } else {
        anchorRef.current = id;
        return false;
      }
      e.preventDefault();
      return true;
    },
    [toggle, rangeTo],
  );

  const onMouseDown = useCallback(
    (down: React.MouseEvent<HTMLDivElement>) => {
      const host = scrollRef.current;
      if (down.button !== 0 || !host) return;
      if ((down.target as HTMLElement).closest(NO_DRAG)) return;
      // No focus ring or text selection from the press; a plain click still fires.
      down.preventDefault();

      const base = down.metaKey || down.ctrlKey || down.shiftKey ? selectedRef.current : [];
      const toContent = (x: number, y: number) => {
        const r = host.getBoundingClientRect();
        return { x: x - r.left + host.scrollLeft, y: y - r.top + host.scrollTop };
      };
      const start = toContent(down.clientX, down.clientY);
      let pointer = { x: down.clientX, y: down.clientY };
      let dragging = false;
      let scrollTimer: number | null = null;

      const update = () => {
        const cur = toContent(pointer.x, pointer.y);
        const rect = {
          left: Math.min(start.x, cur.x),
          top: Math.min(start.y, cur.y),
          width: Math.abs(cur.x - start.x),
          height: Math.abs(cur.y - start.y),
        };
        setMarquee(rect);
        // Hit-test rows in viewport coordinates.
        const hr = host.getBoundingClientRect();
        const left = rect.left - host.scrollLeft + hr.left;
        const top = rect.top - host.scrollTop + hr.top;
        const right = left + rect.width;
        const bottom = top + rect.height;
        const hits = new Set(base);
        host.querySelectorAll<HTMLElement>("[data-slice-id]").forEach((el) => {
          const r = el.getBoundingClientRect();
          if (r.right > left && r.left < right && r.bottom > top && r.top < bottom) {
            hits.add(Number(el.dataset.sliceId));
          }
        });
        setSelected(hits);
      };

      const autoScroll = () => {
        const r = host.getBoundingClientRect();
        const dy =
          pointer.y < r.top + EDGE ? -EDGE_STEP : pointer.y > r.bottom - EDGE ? EDGE_STEP : 0;
        if (dy === 0) return;
        const before = host.scrollTop;
        host.scrollTop += dy;
        if (host.scrollTop !== before) update();
      };

      const onMove = (e: MouseEvent) => {
        pointer = { x: e.clientX, y: e.clientY };
        if (!dragging) {
          const moved = Math.hypot(e.clientX - down.clientX, e.clientY - down.clientY);
          if (moved < DRAG_THRESHOLD) return;
          dragging = true;
          document.body.classList.add("marquee-dragging");
          window.getSelection()?.removeAllRanges();
          scrollTimer = window.setInterval(autoScroll, 30);
        }
        e.preventDefault();
        update();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        if (scrollTimer !== null) window.clearInterval(scrollTimer);
        if (!dragging) return;
        document.body.classList.remove("marquee-dragging");
        setMarquee(null);
        // A drag that ends on the row it started on would also fire that row's click.
        const swallow = (e: MouseEvent) => {
          e.stopPropagation();
          e.preventDefault();
        };
        window.addEventListener("click", swallow, { capture: true, once: true });
        window.setTimeout(() => window.removeEventListener("click", swallow, true), 0);
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [setSelected],
  );

  return {
    selected,
    setSelected,
    clear,
    toggle,
    onRowClick,
    scrollRef,
    onMouseDown,
    marquee,
  };
}

export type SliceSelection = ReturnType<typeof useSliceSelection>;
