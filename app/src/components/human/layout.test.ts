import { describe, expect, it } from "vitest";
import { entryLayout, pageNotesOf, periodLayout, pointText, reportSections } from "./layout";
import { journalOf, node } from "./testing";
import {
  alphaBottom,
  alphaTop,
  dayNote,
  gammaOnly,
  journal,
  lost,
  projects,
  remark,
  report,
  stale,
} from "./fixture";

describe("pointText", () => {
  it("drops the bullet and anchors", () => {
    expect(pointText("- did a thing [#S1]")).toBe("did a thing");
    expect(pointText("  * nested [S12 t3] point")).toBe("nested point");
    expect(pointText("• dot")).toBe("dot");
  });
});

describe("reportSections", () => {
  it("finds `## ` sections and the projects they name, ignoring case and bold", () => {
    const sections = reportSections(report, [{ id: 1, name: "alpha" }]);
    expect(sections.map((s) => [s.heading, s.headingLine, s.project?.id ?? null])).toEqual([
      ["Alpha", 2, 1],
      ["Beta", 7, null],
      ["Misc", 11, null],
    ]);
  });
});

describe("periodLayout", () => {
  const layout = periodLayout(journal, projects, report);
  const [alpha, beta, misc] = layout.sections;

  it("puts notes by project and end", () => {
    expect(layout.notesOf(null)).toEqual([dayNote]);
    expect(layout.notesOf(1)).toEqual([alphaTop, alphaBottom]);
    expect(layout.notesOf(1, "top")).toEqual([alphaTop]);
    expect(layout.notesOf(1, "bottom")).toEqual([alphaBottom]);
  });

  it("puts annotations on their point, or apart when the point is gone", () => {
    expect(layout.annotationsAt("point", 4)).toEqual([remark]);
    expect(layout.annotationsAt("point", 5)).toEqual([]);
    expect(layout.detachedOf(2)).toEqual([stale]);
    expect(layout.looseDetached).toEqual([lost]);
    expect(layout.annotatedPoints(alpha)).toEqual([4]);
    expect(layout.annotatedPoints(beta)).toEqual([]);
  });

  it("lists projects with notes but no section", () => {
    expect(layout.notesOnlyProjects.map((p) => p.id)).toEqual([gammaOnly.projectId]);
  });

  it("tells which sections have anything people wrote", () => {
    expect(layout.sectionHasNotes(alpha)).toBe(true);
    expect(layout.sectionHasNotes(beta)).toBe(true); // only an annotation whose point is gone
    expect(layout.sectionHasNotes(misc)).toBe(false);
  });
});

describe("project page", () => {
  const page = node({ body: "page", periodKind: "project", periodKey: "", projectId: 1 });
  const top = node({ body: "top", projectId: 1, position: "top" });
  const onPoint = node({ body: "on point", projectId: 1, isAnnotation: true });
  const gone = node({ body: "gone", projectId: 1, isAnnotation: true });
  const otherDay = node({ body: "other day", projectId: 1, periodKey: "2026-11-17" });
  const j = journalOf(
    [page, top, onPoint, gone, otherDay],
    [
      { id: onPoint.id, kind: "point", line: 2 },
      { id: gone.id, kind: "detached", line: null },
    ],
  );

  it("keeps the page's own notes apart from period entries", () => {
    expect(pageNotesOf(j)).toEqual([page]);
  });

  it("places an entry's notes and annotations", () => {
    const e = entryLayout(j, "day", "2026-11-16");
    expect(e.top).toEqual([top]);
    expect(e.bottom).toEqual([]);
    expect(e.onPoints).toEqual([onPoint]);
    expect(e.elsewhere).toEqual([gone]);
    expect(e.pointAnnotations(2)).toEqual([onPoint]);
    expect(e.pointAnnotations(3)).toEqual([]);
    expect(e.hasNotes).toBe(true);
    expect(entryLayout(j, "day", "2026-11-18").hasNotes).toBe(false);
    expect(entryLayout(j, "week", "2026-11-16").hasNotes).toBe(false);
  });
});
