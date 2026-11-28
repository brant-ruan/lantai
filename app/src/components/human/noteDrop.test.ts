import { describe, expect, it } from "vitest";
import { dropFor, dropGap, zoneAccepts, type DropRow, type DropZone } from "./noteDrop";

const notes = (
  projectId: number | null,
  period?: string,
  position: "top" | "bottom" = "bottom",
): DropZone => ({ kind: "notes", projectId, position, period });

describe("zoneAccepts", () => {
  it("takes notes of the same project, at either end", () => {
    expect(zoneAccepts(notes(1), notes(1))).toBe(true);
    expect(zoneAccepts(notes(1, undefined, "top"), notes(1))).toBe(true);
    expect(zoneAccepts(notes(1), notes(2))).toBe(false);
    expect(zoneAccepts(notes(null), notes(null))).toBe(true);
  });

  it("keeps blocks in their period where one is given", () => {
    expect(zoneAccepts(notes(1, "2026-11-16"), notes(1, "2026-11-17"))).toBe(false);
    expect(zoneAccepts(notes(1, "2026-11-16"), notes(1))).toBe(true);
  });

  it("takes annotations only on the same point, never among notes", () => {
    const on = (key: string): DropZone => ({ kind: "annotations", key });
    expect(zoneAccepts(on("point:4"), on("point:4"))).toBe(true);
    expect(zoneAccepts(on("point:4"), on("point:6"))).toBe(false);
    expect(zoneAccepts(notes(1), on("point:4"))).toBe(false);
    expect(zoneAccepts(on("point:4"), notes(1))).toBe(false);
  });
});

// Three rows 20 px high: 1, its child 2, then 3 at the top level.
const rows: DropRow[] = [
  { id: 1, depth: 0, top: 0, bottom: 20 },
  { id: 2, depth: 1, top: 20, bottom: 40 },
  { id: 3, depth: 0, top: 40, bottom: 60 },
];

describe("dropGap", () => {
  it("picks the gap above the first row whose middle is below the pointer", () => {
    expect(dropGap(rows, 5, 0, false)).toEqual({ prev: null, after: rows[0], depth: 0 });
    expect(dropGap(rows, 35, 0, false).prev).toBe(rows[1]);
    expect(dropGap(rows, 100, 0, false)).toEqual({ prev: rows[2], after: null, depth: 0 });
  });

  it("keeps the depth between the next row's and one below the previous row", () => {
    // Between 1 and its child 2: only depth 1 keeps 2 under its parent.
    expect(dropGap(rows, 25, 0, false).depth).toBe(1);
    expect(dropGap(rows, 25, 5, false).depth).toBe(1);
    // Between 2 (depth 1) and 3 (depth 0): anything from 0 to 2.
    expect(dropGap(rows, 45, 2, false).depth).toBe(2);
    expect(dropGap(rows, 45, -3, false).depth).toBe(0);
  });

  it("takes the shallowest depth when snapped from outside", () => {
    expect(dropGap(rows, 45, 2, true).depth).toBe(0);
    expect(dropGap(rows, 25, 0, true).depth).toBe(1);
  });

  it("drops at the top level of an empty group", () => {
    expect(dropGap([], 10, 3, false)).toEqual({ prev: null, after: null, depth: 0 });
  });
});

describe("dropFor", () => {
  const parents = new Map<number, number | null>([
    [1, null],
    [2, 1],
    [3, null],
  ]);
  const parentOf = (id: number) => parents.get(id) ?? null;

  it("starts a group, or goes after a row at its own level", () => {
    expect(dropFor(notes(7), null, 0, parentOf, 9)).toEqual({
      type: "notes",
      projectId: 7,
      position: "bottom",
      after: null,
    });
    expect(dropFor(notes(7), rows[2], 0, parentOf, 9)).toEqual({
      type: "notes",
      projectId: 7,
      position: "bottom",
      after: 3,
    });
  });

  it("becomes the first child one level below the previous row", () => {
    expect(dropFor(notes(7), rows[0], 1, parentOf, 9)).toEqual({
      type: "children",
      parent: 1,
      after: null,
    });
  });

  it("goes after the previous row's ancestor at a shallower depth", () => {
    // After child 2, back at the top level: right after its parent 1.
    expect(dropFor(notes(7), rows[1], 0, parentOf, 9)).toEqual({
      type: "notes",
      projectId: 7,
      position: "bottom",
      after: 1,
    });
    // Same depth as child 2: its sibling.
    expect(dropFor(notes(7), rows[1], 1, parentOf, 9)).toEqual({
      type: "children",
      parent: 1,
      after: 2,
    });
  });

  it("moves annotations to the point of the first one moving", () => {
    const zone: DropZone = { kind: "annotations", key: "point:4" };
    expect(dropFor(zone, rows[0], 0, parentOf, 9)).toEqual({
      type: "annotations",
      with: 9,
      after: 1,
    });
  });
});
