// Dragging human notes to reorder them. A drag starts on a block's dot or triangle (a click on
// the triangle still folds it) and carries the block with everything under it, or every
// selected block of the same kind when the block is selected. While it lasts, every group the
// blocks may land in is outlined (an empty end of a project's section shows a placeholder), and
// a line marks where they would go; its indent is the depth they would take. Generated points
// are never a place to land: over them the line snaps to the nearest group of the same card.
//
// Groups declare themselves with a `zone` (see `HumanBlocks`): the notes at one end of a
// project's section (or the day's notes, or a project page's own notes), or the annotations on
// one point. Blocks only move within their period and project, and annotations only among the
// annotations on their point. Which gap and move the pointer stands for is decided in
// `components/human/noteDrop.ts`.
import type React from "react";
import { useSyncExternalStore } from "react";
import type { HumanJournal } from "./useHumanJournal";
import type { HumanDrop, HumanNode } from "../types";
import { setPointerDrag } from "./pointerDrag";
import {
  dropFor,
  dropGap,
  zoneAccepts,
  type DropRow,
  type DropZone,
} from "../components/human/noteDrop";

/** What is being dragged: the blocks moving (with everything under them) and their kind. */
export interface NoteDragState {
  moving: ReadonlySet<number>;
  family: DropZone;
}

/** Movement before a press becomes a drag. */
const THRESHOLD = 4;
/** Distance from the top or bottom edge at which the report scrolls while dragging. */
const EDGE = 40;
/** Horizontal step of one level of nesting in a note outline (see `.human-children`). */
const INDENT = 30;
/** Inner padding of a notes box (see `.human-blocks`). */
const BOX_PAD = 3;
/** What counts as one card: a group snapped to from outside must be in the card under the pointer. */
const CARDS = ".report-section-card, .rollup-entry, .human-day-notes, .human-page-notes";

let state: NoteDragState | null = null;
const listeners = new Set<() => void>();

function setState(next: NoteDragState | null) {
  state = next;
  listeners.forEach((l) => l());
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The drag in progress, if any (blocks re-render only when one starts or ends). */
export function useNoteDragState(): NoteDragState | null {
  return useSyncExternalStore(subscribe, () => state);
}

/** The key of the group an annotation shows in. */
export function annotationKey(journal: HumanJournal, id: number): string {
  const p = journal.placementOf.get(id);
  return p ? `${p.kind}:${p.line ?? ""}` : "";
}

function familyOf(journal: HumanJournal, node: HumanNode): DropZone {
  const period = node.periodKey;
  if (node.parentId === null && node.isAnnotation) {
    return { kind: "annotations", key: annotationKey(journal, node.id), period };
  }
  return { kind: "notes", projectId: node.projectId, position: "bottom", period };
}

function sameFamily(a: DropZone, b: DropZone): boolean {
  return a.period === b.period && zoneAccepts(a, b);
}

interface Target {
  drop: HumanDrop;
  /** The group it lands in. */
  zone: HTMLElement;
  /** The group has no blocks (besides the dragged ones): it is highlighted instead of a line. */
  empty: boolean;
  /** Where to draw the line (viewport coordinates). */
  x: number;
  y: number;
  width: number;
}

/**
 * Starts tracking a press on a block's handle; it becomes a drag once the pointer moves. Call
 * from the handle's `onMouseDown`.
 */
export function startNoteDrag(e: React.MouseEvent, nodeId: number, journal: HumanJournal) {
  if (e.button !== 0 || state) return;
  // Not a box selection, and no text selection either.
  e.stopPropagation();
  e.preventDefault();
  const area = (e.target as HTMLElement).closest<HTMLElement>(".report-content");
  const byId = new Map(journal.nodes.map((n) => [n.id, n]));
  const grabbed = byId.get(nodeId);
  if (!area || !grabbed) return;
  const start = { x: e.clientX, y: e.clientY };
  let pointer = start;
  let active = false;
  let frame = 0;
  let target: Target | null = null;
  let ids: number[] = [];
  let family: DropZone = familyOf(journal, grabbed);
  const line = document.createElement("div");
  line.className = "note-drop-line";

  const parentOf = (id: number) => byId.get(id)?.parentId ?? null;
  const below = (id: number, ancestor: number) => {
    for (let p = parentOf(id); p !== null; p = parentOf(p)) if (p === ancestor) return true;
    return false;
  };

  const begin = () => {
    active = true;
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    // The selected blocks of the same kind, when the grabbed one is selected, in screen order.
    const picked = journal.selected.has(nodeId)
      ? [...area.querySelectorAll<HTMLElement>("[data-note-id]")]
          .map((r) => Number(r.dataset.noteId))
          .filter((id) => journal.selected.has(id))
          .filter((id) => {
            const n = byId.get(id);
            return !!n && sameFamily(familyOf(journal, n), family);
          })
      : [nodeId];
    ids = picked.filter((id) => !picked.some((a) => a !== id && below(id, a)));
    const moving = new Set<number>(
      journal.nodes.filter((n) => ids.some((a) => n.id === a || below(n.id, a))).map((n) => n.id),
    );
    family = familyOf(journal, grabbed);
    setState({ moving, family });
    setPointerDrag(true, "grabbing");
    document.body.appendChild(line);
    frame = requestAnimationFrame(tick);
  };

  const rowsOf = (zone: HTMLElement): DropRow[] => {
    const moving = state?.moving ?? new Set<number>();
    const rows: DropRow[] = [];
    zone.querySelectorAll<HTMLElement>("[data-note-id]").forEach((el) => {
      const id = Number(el.dataset.noteId);
      if (moving.has(id)) return;
      let depth = 0;
      for (let p = parentOf(id); p !== null; p = parentOf(p)) depth += 1;
      const rect = el.getBoundingClientRect();
      rows.push({ id, depth, top: rect.top, bottom: rect.bottom });
    });
    return rows;
  };

  const zoneData = (el: HTMLElement): DropZone => JSON.parse(el.dataset.dropZone ?? "{}");

  const locate = (): Target | null => {
    const zones = [...document.querySelectorAll<HTMLElement>("[data-drop-zone]")].filter((z) =>
      zoneAccepts(zoneData(z), family),
    );
    if (zones.length === 0) return null;
    const inside = (r: DOMRect, pad: number) =>
      pointer.x >= r.left - pad &&
      pointer.x <= r.right + pad &&
      pointer.y >= r.top - pad &&
      pointer.y <= r.bottom + pad;
    let zoneEl = zones.find((z) => inside(z.getBoundingClientRect(), 8)) ?? null;
    // Snapped from outside a group: the blocks land at its top level.
    const snapped = !zoneEl;
    if (!zoneEl) {
      // Over a card but not over a group (its generated points, its header): the nearest
      // group of that card.
      // (By rectangle: report content is out of hit testing during a drag.)
      const card = [...document.querySelectorAll<HTMLElement>(CARDS)]
        .filter((c) => inside(c.getBoundingClientRect(), 0))
        .reduce<HTMLElement | null>((a, b) => (a && b.contains(a) ? a : b), null);
      const mine = zones.filter((z) => card?.contains(z));
      if (mine.length === 0) return null;
      const dist = (z: HTMLElement) => {
        const r = z.getBoundingClientRect();
        return pointer.y < r.top ? r.top - pointer.y : pointer.y > r.bottom ? pointer.y - r.bottom : 0;
      };
      zoneEl = mine.reduce((a, b) => (dist(b) < dist(a) ? b : a));
    }
    const zone = zoneData(zoneEl);
    const box = zoneEl.getBoundingClientRect();
    const rows = rowsOf(zoneEl);
    const wanted = Math.round((pointer.x - (box.left + BOX_PAD + 17)) / INDENT);
    const { prev, after, depth } = dropGap(rows, pointer.y, wanted, snapped);
    const y = prev && after
      ? (prev.bottom + after.top) / 2
      : prev
        ? prev.bottom + 1
        : after
          ? after.top - 1
          : box.top + box.height / 2;
    const x = box.left + BOX_PAD + 3 + depth * INDENT;
    return {
      drop: dropFor(zone, prev, depth, parentOf, ids[0]),
      zone: zoneEl,
      empty: rows.length === 0,
      x,
      y,
      width: box.right - BOX_PAD - x,
    };
  };

  const update = () => {
    target?.zone.classList.remove("is-drop-over");
    target = locate();
    target?.zone.classList.add("is-drop-over");
    if (target && !target.empty) {
      line.style.display = "block";
      line.style.left = `${target.x}px`;
      line.style.top = `${target.y - 1}px`;
      line.style.width = `${Math.max(24, target.width)}px`;
    } else {
      line.style.display = "none";
    }
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
      if (Math.hypot(pointer.x - start.x, pointer.y - start.y) < THRESHOLD) return;
      begin();
    }
    update();
  };

  const swallowClick = (ev: MouseEvent) => {
    ev.stopPropagation();
    ev.preventDefault();
  };

  const finish = (drop: boolean) => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
    window.removeEventListener("keydown", onKey, true);
    if (!active) return;
    cancelAnimationFrame(frame);
    line.remove();
    target?.zone.classList.remove("is-drop-over");
    setPointerDrag(false);
    setState(null);
    // The click that ends a drag must not fold the block under the pointer.
    window.addEventListener("click", swallowClick, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallowClick, { capture: true }));
    if (drop && target) void journal.moveTo(ids, target.drop);
  };

  const onUp = () => finish(true);
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key !== "Escape") return;
    ev.preventDefault();
    ev.stopPropagation();
    finish(false);
  };

  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
  window.addEventListener("keydown", onKey, true);
}
