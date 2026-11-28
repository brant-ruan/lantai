// A day's report with notes of every kind, shared by the layout and copy tests.
import type { HumanPlacement } from "../../types";
import { journalOf, node } from "./testing";

export const projects = [
  { id: 1, name: "Alpha" },
  { id: 2, name: "Beta" },
  { id: 3, name: "Gamma" },
];

export const report = [
  "# 2026-11-16 · Mon", // 0
  "", // 1
  "## **Alpha**", // 2
  "", // 3
  "- did a thing [#S1]", // 4
  "  - detail", // 5
  "", // 6
  "## Beta", // 7
  "", // 8
  "- other thing", // 9
  "", // 10
  "## Misc", // 11
  "- misc", // 12
];

export const dayNote = node({ body: "day note" });
export const alphaTop = node({ body: "before alpha", projectId: 1, position: "top" });
export const alphaBottom = node({ body: "after alpha", projectId: 1 });
export const nested = node({
  body: "nested\nsecond line",
  projectId: 1,
  parentId: alphaBottom.id,
});
export const remark = node({ body: "remark", projectId: 1, isAnnotation: true });
export const stale = node({
  body: "stale",
  projectId: 2,
  isAnnotation: true,
  quote: "old point",
});
export const gammaOnly = node({ body: "gamma only", projectId: 3 });
/** An annotation whose point is gone and whose project is unknown (no placement either). */
export const lost = node({ body: "lost", isAnnotation: true });

const placements: HumanPlacement[] = [
  { id: remark.id, kind: "point", line: 4 },
  { id: stale.id, kind: "detached", line: null },
];

export const journal = journalOf(
  [dayNote, alphaTop, alphaBottom, nested, remark, stale, gammaOnly, lost],
  placements,
);

export const labels = {
  dayNotes: "Day notes",
  pageNotes: "Page notes",
  otherAnnotations: "Other annotations",
  pointChanged: "Point changed",
};
