// Where dragged human notes land, as pure functions of the rows on screen: which groups accept
// them, which gap between rows the pointer is over and at what depth, and the move that gap
// stands for. `useNoteDrag` measures the page and draws; this decides.
import type { HumanDrop, HumanPosition } from "../../types";

/** A group blocks can be dropped into. */
export type DropZone =
  | {
      kind: "notes";
      projectId: number | null;
      position: HumanPosition;
      /** The period the group belongs to, where a view shows several (a project page). */
      period?: string;
    }
  | { kind: "annotations"; key: string; period?: string };

/** Whether blocks of `family` may be dropped in `zone`. */
export function zoneAccepts(zone: DropZone, family: DropZone): boolean {
  if (zone.period !== undefined && family.period !== undefined && zone.period !== family.period) {
    return false;
  }
  if (zone.kind === "notes" && family.kind === "notes") return zone.projectId === family.projectId;
  if (zone.kind === "annotations" && family.kind === "annotations") return zone.key === family.key;
  return false;
}

/** A block of a group as laid out, in screen order; `depth` is its nesting (0 at the top). */
export interface DropRow {
  id: number;
  depth: number;
  top: number;
  bottom: number;
}

/** The gap the pointer is over: the rows around it and the depth the blocks would take. */
export interface DropGap {
  prev: DropRow | null;
  after: DropRow | null;
  depth: number;
}

/**
 * The gap above the first row whose middle is below `pointerY` (the end when none is). The
 * depth is `wantedDepth` kept between the next row's depth and one deeper than the previous
 * row's; `snapped` (the pointer came from outside the group) takes the shallowest.
 */
export function dropGap(
  rows: DropRow[],
  pointerY: number,
  wantedDepth: number,
  snapped: boolean,
): DropGap {
  const next = rows.findIndex((r) => (r.top + r.bottom) / 2 > pointerY);
  const gap = next === -1 ? rows.length : next;
  const prev = gap > 0 ? rows[gap - 1] : null;
  const after = gap < rows.length ? rows[gap] : null;
  const min = after ? after.depth : 0;
  const max = prev ? prev.depth + 1 : 0;
  const depth = snapped ? min : Math.max(min, Math.min(max, wantedDepth));
  return { prev, after, depth };
}

/**
 * The move for dropping into `zone` after `prev` (none: the start of the group) at `depth`.
 * `parentOf` gives a block's parent; `first` is the first block moving (annotations move to
 * the point it is on).
 */
export function dropFor(
  zone: DropZone,
  prev: DropRow | null,
  depth: number,
  parentOf: (id: number) => number | null,
  first: number,
): HumanDrop {
  if (prev && depth === prev.depth + 1) return { type: "children", parent: prev.id, after: null };
  // The ancestor of `prev` at `depth`: the block the dragged ones go right after.
  let after: number | null = null;
  if (prev) {
    let id = prev.id;
    for (let d = prev.depth; d > depth; d -= 1) id = parentOf(id) ?? id;
    after = id;
  }
  if (depth > 0 && after !== null) {
    return { type: "children", parent: parentOf(after)!, after };
  }
  if (zone.kind === "notes") {
    return { type: "notes", projectId: zone.projectId, position: zone.position, after };
  }
  return { type: "annotations", with: first, after };
}
