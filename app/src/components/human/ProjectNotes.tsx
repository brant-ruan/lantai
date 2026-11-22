// The human journal on a project page: the page's own notes at the top, and for each period
// entry the project's notes of that period (before or after its text, as in the period's
// report) and the annotations on its points. They are the same nodes the period's report
// shows, edited in place.
import React from "react";
import { useT } from "../../i18n";
import type { HumanJournal } from "../../hooks/useHumanJournal";
import type { HumanNode, HumanPosition, RollupGranularity } from "../../types";
import { entryLayout, pageNotesOf, PERIOD_KIND } from "./layout";
import { HumanBlocks } from "./HumanBlocks";
import type { JournalLinkHandlers } from "./HumanMarkdown";
import type { DropZone } from "../../hooks/useNoteDrag";
import {
  AddNoteButton,
  AnnotateButton,
  HeaderAddNote,
  OrphanedAnnotations,
} from "./parts";

export interface ProjectNotes {
  /** The page's own notes (nothing when there are none). */
  pageNotes: React.ReactNode;
  addPageNote: () => void;
  /** Whether a period has notes or annotations of the project. */
  hasNotes: (period: string) => boolean;
  pointExtras: (period: string) => (line: number) => React.ReactNode;
  pointAction: (period: string) => (line: number) => React.ReactNode;
  /** "+ Note" for a period entry's header (its menu adds at the top or the bottom). */
  entryAddNote: (period: string) => React.ReactNode;
  /** The period's notes at the top of its entry (nothing when there are none). */
  entryTopNotes: (period: string) => React.ReactNode;
  /**
   * The period's notes at the bottom, with annotations whose point is gone; with `withContext`, also every annotation with the text of its point (when the
   * report text is hidden).
   */
  entryNotes: (period: string, withContext: boolean) => React.ReactNode;
}

export function useProjectNotes({
  journal,
  projectId,
  granularity,
  links,
}: {
  journal: HumanJournal;
  projectId: number;
  granularity: RollupGranularity;
  links: JournalLinkHandlers;
}): ProjectNotes {
  const t = useT().human;
  const kind = PERIOD_KIND[granularity];
  const entry = (period: string) => entryLayout(journal, kind, period);
  const placed = (n: HumanNode) => journal.placementOf.get(n.id);
  const blocks = (list: HumanNode[], zone?: DropZone) => (
    <HumanBlocks roots={list} journal={journal} zone={zone} {...links} />
  );
  const notesZone = (period: string, position: HumanPosition): DropZone => ({
    kind: "notes",
    projectId,
    position,
    period,
  });
  const pointZone = (period: string, line: number | null | undefined): DropZone => ({
    kind: "annotations",
    key: `point:${line ?? ""}`,
    period,
  });

  const page = pageNotesOf(journal);
  const addPageNote = () => void journal.create({ type: "projectNote", projectId });

  return {
    pageNotes: page.length > 0 && (
      <section className="human-day-notes human-page-notes">
        <div className="human-day-head" title={t.pageNotesTip}>
          {t.pageNotes}
        </div>
        {blocks(page, notesZone(String(projectId), "bottom"))}
        <AddNoteButton label={t.addNote} onClick={addPageNote} />
      </section>
    ),
    addPageNote,
    hasNotes: (period) => entry(period).hasNotes,
    pointExtras: (period) => (line) => {
      const list = entry(period).pointAnnotations(line);
      return list.length > 0 ? (
        <div className="human-annotations">{blocks(list, pointZone(period, line))}</div>
      ) : null;
    },
    pointAction: (period) => (line) => (
      <AnnotateButton
        onClick={() => void journal.create({ type: "annotation", line, period })}
      />
    ),
    entryAddNote: (period) => (
      <HeaderAddNote
        onAdd={(position) =>
          void journal.create({ type: "projectNote", projectId, period, position })
        }
      />
    ),
    // Rendered even without notes: while a note is dragged it shows a place to drop it.
    entryTopNotes: (period) => (
      <div className="human-section-top">
        {blocks(entry(period).top, notesZone(period, "top"))}
      </div>
    ),
    entryNotes: (period, withContext) => {
      const { onPoints, elsewhere, bottom: notes } = entry(period);
      return (
        <div className="human-section-notes">
          {withContext &&
            onPoints.map((n) => (
              <div key={n.id} className="human-context">
                <div className="human-context-point">{n.quote}</div>
                {blocks([n], pointZone(period, placed(n)?.line))}
              </div>
            ))}
          <OrphanedAnnotations list={elsewhere} journal={journal} {...links} />
          {blocks(notes, notesZone(period, "bottom"))}
        </div>
      );
    },
  };
}
