// Places the human journal around a generated report: day notes above it, annotations under
// the points they were written on, project notes at the top or the end of each project
// section (the header's "+ Note" asks which), cards for
// projects only people wrote about, and an "only human" view that shows nothing else. New notes
// start from one menu in the report header; nothing is shown in the body until there is a note.
import React, { useEffect, useMemo } from "react";
import { readItem, storeItem } from "../../hooks/storage";
import { NoteAddIcon } from "../icons";
import { useT } from "../../i18n";
import type { HumanJournal } from "../../hooks/useHumanJournal";
import type { HumanNode, HumanPosition } from "../../types";
import { DropdownMenu } from "../DropdownMenu";
import { HumanBlocks } from "./HumanBlocks";
import {
  AddNoteButton,
  AnnotateButton,
  HeaderAddNote,
  OrphanedAnnotations,
} from "./parts";
import type { JournalLinkHandlers } from "./HumanMarkdown";
import type { DropZone } from "../../hooks/useNoteDrag";
import { assignableProjects } from "../../projectOptions";
import { periodLayout, pointText, type ProjectRef } from "./layout";

const HINT_KEY = "lantai.human.hintSeen";

export interface HumanLayer {
  /** The "add a note" menu for the report header. */
  addMenu: React.ReactNode;
  /** Whether the period has any note or annotation. */
  hasNotes: boolean;
  /** The period's notes that belong to no project (nothing when there are none). */
  dayNotes: React.ReactNode;
  pointExtras: (line: number) => React.ReactNode;
  pointAction: (line: number) => React.ReactNode;
  /** A project section's notes at its top. */
  sectionTop: (heading: string, headingLine: number) => React.ReactNode;
  /** A project section's notes at its end. */
  sectionFooter: (heading: string, headingLine: number) => React.ReactNode;
  /** "+ Note" for the header of a project's section (nothing for other headings). */
  sectionAddNote: (heading: string) => React.ReactNode;
  /** Cards for projects with notes but no generated section. */
  afterSections: React.ReactNode;
  /** Everything people wrote, with the annotated points as faint context. */
  humanOnly: React.ReactNode;
}

export function useHumanLayer({
  journal,
  projects,
  markdown,
  links,
}: {
  journal: HumanJournal;
  /** All projects, in the user's order. */
  projects: ProjectRef[];
  /** The report version on screen, as displayed ("" when there is none). */
  markdown: string;
  links: JournalLinkHandlers;
}): HumanLayer {
  const t = useT().human;
  const lines = useMemo(() => markdown.split("\n"), [markdown]);
  const layout = periodLayout(journal, projects, lines);
  const { sections, notesOf, annotationsAt, detachedOf } = layout;

  const blocks = (list: HumanNode[], zone?: DropZone) => (
    <HumanBlocks roots={list} journal={journal} zone={zone} {...links} />
  );
  const notesZone = (projectId: number | null, position: HumanPosition): DropZone => ({
    kind: "notes",
    projectId,
    position,
  });
  const pointZone = (line: number): DropZone => ({ kind: "annotations", key: `point:${line}` });

  const addDayNote = (
    <AddNoteButton
      label={t.addDayNote}
      onClick={() => void journal.create({ type: "dayNote" })}
    />
  );

  const orphaned = (list: HumanNode[]) => (
    <OrphanedAnnotations list={list} journal={journal} {...links} />
  );

  // Rendered even without notes: while a note of the project is dragged it shows a place to
  // drop it.
  const topNotes = (project: ProjectRef | null) =>
    project && (
      <div className="human-section-top">
        {blocks(notesOf(project.id, "top"), notesZone(project.id, "top"))}
      </div>
    );

  const projectNotes = (project: ProjectRef | null, headingLine: number | null) => (
    <div className="human-section-notes">
      {headingLine !== null && orphaned(annotationsAt("section", headingLine))}
      {project && orphaned(detachedOf(project.id))}
      {project && blocks(notesOf(project.id, "bottom"), notesZone(project.id, "bottom"))}
    </div>
  );

  const addNoteFor = (project: ProjectRef | null) =>
    project && (
      <HeaderAddNote
        onAdd={(position) =>
          void journal.create({ type: "projectNote", projectId: project.id, position })
        }
      />
    );

  const card = (key: string, title: string, body: React.ReactNode, project: ProjectRef | null) => (
    <section key={key} className="report-section-card human-card">
      <div className="report-section-head">
        <span className="section-accent-bar is-human" />
        <h2>{title}</h2>
        <div className="report-section-action">{addNoteFor(project)}</div>
      </div>
      <div className="report-section-body">{body}</div>
    </section>
  );

  const hasNotes = journal.nodes.length > 0;
  // Until the first note is written on this machine, the menu also shows its name.
  const hintSeen = hasNotes || readItem(HINT_KEY) === "1";
  useEffect(() => {
    if (hasNotes) storeItem(HINT_KEY, "1");
  }, [hasNotes]);

  const addMenu = (
    <DropdownMenu
      label={
        <span className="human-add-menu-label">
          <NoteAddIcon />
          {!hintSeen && <span>{t.addMenuHint}</span>}
        </span>
      }
      tip={hintSeen ? t.addMenu : undefined}
      ariaLabel={t.addMenu}
      variant="ghost"
      size="xs"
      hideChevron
      buttonClassName="human-add-menu"
    >
      {(close) => (
        <div className="dropdown-form human-project-menu">
          <button
            type="button"
            className="dropdown-item"
            onClick={() => {
              close();
              void journal.create({ type: "dayNote" });
            }}
          >
            {t.generalNote}
          </button>
          <div className="dropdown-sep" />
          <div className="dropdown-section-label">{t.onProject}</div>
          {assignableProjects(projects).map((p) => (
            <button
              key={p.id}
              type="button"
              className="dropdown-item"
              onClick={() => {
                close();
                void journal.create({ type: "projectNote", projectId: p.id });
              }}
            >
              {p.name}
            </button>
          ))}
        </div>
      )}
    </DropdownMenu>
  );

  const humanOnlyCards = (
    <>
      {layout.notesOnlyProjects.map((p) =>
        card(
          `hp-${p.id}`,
          p.name,
          <>
            {topNotes(p)}
            {projectNotes(p, null)}
          </>,
          p,
        ),
      )}
      {layout.looseDetached.length > 0 &&
        card("hp-loose", t.otherAnnotations, orphaned(layout.looseDetached), null)}
    </>
  );

  const dayNotes = notesOf(null).length > 0 && (
    <section className="human-day-notes">
      <div className="human-day-head">{t.dayNotes}</div>
      {blocks(notesOf(null), notesZone(null, "bottom"))}
      {addDayNote}
    </section>
  );

  const humanOnly = (() => {
    const cards = sections
      .map((s) => {
        if (!layout.sectionHasNotes(s)) return null;
        return card(
          `ho-${s.headingLine}`,
          s.heading,
          <>
            {topNotes(s.project)}
            {layout.annotatedPoints(s).map((i) => (
              <div key={i} className="human-context">
                <div className="human-context-point">{pointText(lines[i])}</div>
                {blocks(annotationsAt("point", i), pointZone(i))}
              </div>
            ))}
            {projectNotes(s.project, s.headingLine)}
          </>,
          s.project,
        );
      })
      .filter(Boolean);
    return (
      <div className="report-sections">
        {dayNotes}
        {cards}
        {humanOnlyCards}
      </div>
    );
  })();

  return {
    addMenu,
    hasNotes,
    dayNotes,
    pointExtras: (line) => {
      const list = annotationsAt("point", line);
      return list.length > 0 ? (
        <div className="human-annotations">{blocks(list, pointZone(line))}</div>
      ) : null;
    },
    pointAction: (line) => (
      <AnnotateButton onClick={() => void journal.create({ type: "annotation", line })} />
    ),
    sectionTop: (_heading, headingLine) =>
      topNotes(sections.find((x) => x.headingLine === headingLine)?.project ?? null),
    sectionFooter: (_heading, headingLine) =>
      projectNotes(sections.find((x) => x.headingLine === headingLine)?.project ?? null, headingLine),
    sectionAddNote: (heading) =>
      addNoteFor(sections.find((s) => s.heading === heading)?.project ?? null),
    afterSections: humanOnlyCards,
    humanOnly,
  };
}
