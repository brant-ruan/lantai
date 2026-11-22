// Where the human journal goes around the agent's text, as plain data: the notes of a report
// period by project section and end, the annotations by point, and the notes of each period
// entry on a project page. The views render it and copying turns it into Markdown
// (`journalMarkdown.ts`), so the two always agree.
import type { HumanNode, HumanPlacement, HumanPosition, RollupGranularity } from "../../types";

export interface ProjectRef {
  id: number;
  name: string;
  archived?: boolean;
}

/** The nodes of the view on screen (the part of `HumanJournal` that says where they are). */
export interface JournalNodes {
  /** Children of each node, and roots under key `null`, in order. */
  childrenOf: Map<number | null, HumanNode[]>;
  placementOf: Map<number, HumanPlacement>;
}

export interface Section {
  heading: string;
  headingLine: number;
  project: ProjectRef | null;
}

/** The text of a report point without bullet and anchors, for showing it as context. */
export function pointText(line: string): string {
  return line
    .replace(/^\s*(?:[-*]|•)\s+/, "")
    .replace(/\s*\[#?S\d+[^\]]*\]/g, "")
    .trim();
}

/** The `## ` sections of a report, each with the project it names (if any). */
export function reportSections(lines: string[], projects: ProjectRef[]): Section[] {
  const byName = new Map(projects.map((p) => [p.name.toLowerCase(), p]));
  const out: Section[] = [];
  lines.forEach((l, i) => {
    const m = /^\s*## (.+)$/.exec(l);
    if (!m) return;
    const heading = m[1].replace(/\*\*/g, "").trim();
    out.push({ heading, headingLine: i, project: byName.get(heading.toLowerCase()) ?? null });
  });
  return out;
}

export interface PeriodLayout {
  sections: Section[];
  /** Root notes of a project (or of the period when `null`), at one end of its section or both. */
  notesOf: (projectId: number | null, position?: HumanPosition) => HumanNode[];
  /** Annotations under the point on `line`, or at the end of the section headed on `line`. */
  annotationsAt: (kind: "point" | "section", line: number) => HumanNode[];
  /** A project's annotations whose point is gone. */
  detachedOf: (projectId: number) => HumanNode[];
  /** Projects with notes but no section in the report, in the user's order. */
  notesOnlyProjects: ProjectRef[];
  /** Annotations whose point is gone and whose project is unknown. */
  looseDetached: HumanNode[];
  /** The lines of a section's points that carry annotations. */
  annotatedPoints: (section: Section) => number[];
  /** Whether a section has anything people wrote (the "only human" view shows only those). */
  sectionHasNotes: (section: Section) => boolean;
}

export function periodLayout(
  journal: JournalNodes,
  projects: ProjectRef[],
  lines: string[],
): PeriodLayout {
  const sections = reportSections(lines, projects);
  const roots = journal.childrenOf.get(null) ?? [];
  const notesOf = (projectId: number | null, position?: HumanPosition) =>
    roots.filter(
      (n) =>
        !n.isAnnotation &&
        n.projectId === projectId &&
        (position === undefined || n.position === position),
    );
  const annotationsAt = (kind: "point" | "section", line: number) =>
    roots.filter((n) => {
      const p = journal.placementOf.get(n.id);
      return n.isAnnotation && p?.kind === kind && p.line === line;
    });
  const detached = roots.filter(
    (n) => n.isAnnotation && (journal.placementOf.get(n.id)?.kind ?? "detached") === "detached",
  );
  const detachedOf = (projectId: number) => detached.filter((n) => n.projectId === projectId);

  const shown = new Set(sections.map((s) => s.project?.id).filter((x) => x !== undefined));
  const notesOnlyProjects = projects.filter(
    (p) => !shown.has(p.id) && (notesOf(p.id).length > 0 || detachedOf(p.id).length > 0),
  );
  const looseDetached = detached.filter(
    (n) => n.projectId === null || !projects.some((p) => p.id === n.projectId),
  );

  const annotatedPoints = (s: Section) => {
    const end = sections.find((x) => x.headingLine > s.headingLine)?.headingLine ?? lines.length;
    const out: number[] = [];
    for (let i = s.headingLine + 1; i < end; i++) {
      if (annotationsAt("point", i).length > 0) out.push(i);
    }
    return out;
  };
  const sectionHasNotes = (s: Section) =>
    annotatedPoints(s).length > 0 ||
    annotationsAt("section", s.headingLine).length > 0 ||
    (s.project !== null &&
      (notesOf(s.project.id).length > 0 || detachedOf(s.project.id).length > 0));

  return {
    sections,
    notesOf,
    annotationsAt,
    detachedOf,
    notesOnlyProjects,
    looseDetached,
    annotatedPoints,
    sectionHasNotes,
  };
}

/** The journal kind of a project page's period entries. */
export const PERIOD_KIND: Record<RollupGranularity, string> = {
  daily: "day",
  weekly: "week",
  monthly: "month",
};

/** A project page's own notes (they belong to no period). */
export function pageNotesOf(journal: JournalNodes): HumanNode[] {
  return (journal.childrenOf.get(null) ?? []).filter((n) => n.periodKind === "project");
}

/** The project's nodes of one period entry on its project page. */
export interface EntryLayout {
  /** Notes before the period's text. */
  top: HumanNode[];
  /** Notes after it. */
  bottom: HumanNode[];
  /** Annotations shown under a point of the text. */
  onPoints: HumanNode[];
  /** Annotations whose point is gone. */
  elsewhere: HumanNode[];
  /** The annotations under the point on `line` of the entry's text. */
  pointAnnotations: (line: number) => HumanNode[];
  hasNotes: boolean;
}

export function entryLayout(journal: JournalNodes, periodKind: string, period: string): EntryLayout {
  const list = (journal.childrenOf.get(null) ?? []).filter(
    (n) => n.periodKind === periodKind && n.periodKey === period,
  );
  const placed = (n: HumanNode) => journal.placementOf.get(n.id);
  const annotations = list.filter((n) => n.isAnnotation);
  const notes = list.filter((n) => !n.isAnnotation);
  return {
    top: notes.filter((n) => n.position === "top"),
    bottom: notes.filter((n) => n.position === "bottom"),
    onPoints: annotations.filter((n) => placed(n)?.kind === "point"),
    elsewhere: annotations.filter((n) => placed(n)?.kind !== "point"),
    pointAnnotations: (line) =>
      annotations.filter((n) => placed(n)?.kind === "point" && placed(n)?.line === line),
    hasNotes: list.length > 0,
  };
}
