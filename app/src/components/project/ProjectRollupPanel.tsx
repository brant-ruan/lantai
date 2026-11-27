import React, { useEffect, useMemo, useState } from "react";
import {
  MarkdownReport,
  hasCollapsibleItems,
  type AnchorClickHandler,
} from "../../anchors";
import type {
  ExportResult,
  ProjectRangeKey,
  ProjectRollup,
  RollupGranularity,
} from "../../types";
import { useT } from "../../i18n";
import { readItem, storeItem } from "../../storage";
import { ExportModal } from "../report/ExportModal";
import { detailsToggle, useNoteFolds } from "../../hooks/useDetailsToggle";
import { OutlineToggle } from "../report/OutlineToggle";
import { ReportCanvas } from "../report/ReportCanvas";
import { CopyButton } from "../CopyButton";
import { SortIcon } from "../icons";
import { ArchivedBanner, ProjectMenu, type ProjectMenuActions } from "./ProjectMenu";
import { RangeSelect } from "./RangeSelect";
import { useProjectNotes } from "../human/ProjectNotes";
import { useHumanJournal } from "../../hooks/useHumanJournal";
import { useNoteMarquee } from "../../hooks/useNoteMarquee";
import { NoteAddIcon } from "../icons";
import { SourceFilterPills, useSourceFilter } from "../human/SourceFilter";
import { rollupMarkdown } from "../human/journalMarkdown";
import { pageNotesOf, PERIOD_KIND } from "../human/layout";

interface ProjectRollupPanelProps {
  rollup: ProjectRollup;
  workspaceSwitcher?: React.ReactNode;
  range: ProjectRangeKey;
  onChangeRange: (range: ProjectRangeKey) => void;
  onChangeGranularity: (granularity: RollupGranularity) => void;
  onExport: (params: {
    markdown: string;
    customPath: string | null;
    anchorMode: "keep" | "footnotes" | "strip";
    force: boolean;
  }) => Promise<ExportResult>;
  /** Open the daily or weekly report of a period key. */
  onJumpToPeriod: (period: string) => void;
  /** Open a day or week named in a note. */
  onOpenPeriod?: (key: string) => void;
  menu: ProjectMenuActions;
  onSelectAnchor: AnchorClickHandler;
  onError: (message: string) => void;
  /** After a change that may add or remove a note of a period (the sidebar counts them). */
  onNotesChanged?: () => void;
}

const GRANULARITIES: RollupGranularity[] = ["daily", "weekly", "monthly"];

export function ProjectRollupPanel({
  rollup,
  workspaceSwitcher,
  range,
  onChangeRange,
  onChangeGranularity,
  onExport,
  onJumpToPeriod,
  onOpenPeriod,
  onSelectAnchor,
  menu,
  onError,
  onNotesChanged,
}: ProjectRollupPanelProps) {
  const tr = useT();
  const t = tr.rollup;
  const [sourceFilter, chooseSourceFilter] = useSourceFilter();
  const showAgent = sourceFilter !== "human";
  const showHuman = sourceFilter !== "agent";
  const [exportOpen, setExportOpen] = useState(false);
  // The page's own notes and the project's notes of every period shown, edited in place.
  const journal = useHumanJournal(
    { type: "project", projectId: rollup.projectId, granularity: rollup.granularity },
    rollup.entries.map((e) => `${e.period}@${e.updatedAt}`).join(","),
    onError,
    onNotesChanged,
  );
  const marquee = useNoteMarquee(journal, showHuman);
  const notes = useProjectNotes({
    journal,
    projectId: rollup.projectId,
    granularity: rollup.granularity,
    links: { onSelectAnchor, onOpenPeriod },
  });
  const [expandCmd, setExpandCmd] = useState<{
    expandAll: boolean;
    seq: number;
  }>({ expandAll: false, seq: 0 });

  useEffect(() => {
    setExportOpen(false);
    setExpandCmd({ expandAll: false, seq: 0 });
  }, [rollup.projectId, rollup.periodKey, rollup.granularity]);

  const g = rollup.granularity;

  const [oldestFirst, setOldestFirst] = useState<boolean>(
    () => readItem("rollup-oldest-first") === "1",
  );
  const toggleOrder = () =>
    setOldestFirst((prev) => {
      storeItem("rollup-oldest-first", prev ? "0" : "1");
      return !prev;
    });

  const sortedEntries = useMemo(() => {
    const list = rollup.entries.filter(
      (e) => (showAgent && e.markdown.trim() !== "") || (showHuman && notes.hasNotes(e.period)),
    );
    list.sort((a, b) =>
      oldestFirst
        ? a.period.localeCompare(b.period)
        : b.period.localeCompare(a.period),
    );
    return list;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rollup.entries, oldestFirst, showAgent, showHuman, journal.nodes]);

  // Export carries the report text only; notes are left out until export offers them.
  const hasReportText = rollup.entries.some((e) => e.markdown.trim() !== "");
  // Copy follows the source filter and the order on screen.
  const canCopy =
    sortedEntries.length > 0 || (showHuman && pageNotesOf(journal).length > 0);
  const copyText = () =>
    rollupMarkdown({
      title: rollup.combinedMarkdown.split("\n")[0],
      entries: sortedEntries,
      periodKind: PERIOD_KIND[rollup.granularity],
      journal,
      filter: sourceFilter,
      labels: tr.human,
    });

  const anyCollapsible = useMemo(
    () => rollup.entries.some((entry) => hasCollapsibleItems(entry.markdown)),
    [rollup.entries],
  );
  const noteFolds = useNoteFolds(journal, showHuman);
  const details = detailsToggle([
    {
      present: showAgent && anyCollapsible,
      expanded: expandCmd.expandAll,
      setExpanded: (value) => setExpandCmd((prev) => ({ expandAll: value, seq: prev.seq + 1 })),
    },
    noteFolds,
  ]);

  return (
    <section className="panel report-panel">
      <header className="panel-header">
        <div className="topbar-left">{workspaceSwitcher}</div>

        <div className="topbar-right">
          <SourceFilterPills value={sourceFilter} onChange={chooseSourceFilter} />
          <div className="pill-group" role="radiogroup" aria-label={t.summariseBy}>
            {GRANULARITIES.map((gr) => (
              <button
                key={gr}
                type="button"
                role="radio"
                aria-checked={rollup.granularity === gr}
                className={`pill ${rollup.granularity === gr ? "active" : ""}`}
                data-tip={tr.granularity.tip[gr]}
                onClick={() => onChangeGranularity(gr)}
              >
                {tr.granularity.short[gr]}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="btn btn-ghost toolbar-icon-btn"
            onClick={toggleOrder}
            aria-label={oldestFirst ? t.oldestFirstTitle : t.newestFirstTitle}
            data-tip={oldestFirst ? t.oldestFirstTitle : t.newestFirstTitle}
          >
            <SortIcon oldestFirst={oldestFirst} />
          </button>
          <RangeSelect value={range} onChange={onChangeRange} />
          <ProjectMenu actions={menu}>
            {(close) => (
              <>
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={!hasReportText}
                  onClick={() => {
                    setExportOpen(true);
                    close();
                  }}
                >
                  {tr.report.exportMarkdown}
                </button>
                <div className="dropdown-sep" />
              </>
            )}
          </ProjectMenu>
        </div>
      </header>

      <ExportModal
        open={exportOpen && hasReportText}
        title={t.exportTitle(rollup.projectName)}
        defaultExportPath={rollup.defaultExportPath}
        markdown={rollup.combinedMarkdown}
        onClose={() => setExportOpen(false)}
        onExport={onExport}
      />

      <ReportCanvas>
        <div className="report-document">
          <div className="markdown-body rollup-doc-head">
            <header className="report-doc-header">
              <h1 className="report-doc-title">{rollup.projectName}</h1>
              {(canCopy || showHuman || details.visible) && (
                <div className="report-doc-actions">
                  <div className="report-doc-action-row">
                    {showHuman && (
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs human-add-menu"
                        data-tip={tr.human.addPageNote}
                        aria-label={tr.human.addPageNote}
                        onClick={notes.addPageNote}
                      >
                        <NoteAddIcon />
                      </button>
                    )}
                    {canCopy && <CopyButton text={copyText} />}
                    {details.visible && (
                      <OutlineToggle expanded={details.expanded} onToggle={details.toggle} />
                    )}
                  </div>
                </div>
              )}
            </header>
          </div>

          <ArchivedBanner actions={menu} />
          {showHuman && notes.pageNotes}
          {rollup.unreportedPeriods.length > 0 && (
            <div className="banner banner-info rollup-unreported">
              <span>
                {t.unreported(g, rollup.projectName)}
              </span>
              <div className="rollup-date-pills">
                {rollup.unreportedPeriods.map((d) => (
                  <button
                    key={d}
                    type="button"
                    className="date-chip"
                    onClick={() => onJumpToPeriod(d)}
                    title={t.openWorkbench(g, d)}
                  >
                    {d}
                  </button>
                ))}
              </div>
            </div>
          )}

          {sortedEntries.length === 0 ? (
            <div className="empty-card">
              <div className="empty-card-title">
                {t.noEntries(g, rollup.projectName)}
              </div>
              <div className="muted-line">
                {rollup.unreportedPeriods.length > 0
                  ? t.pickToGenerate(g)
                  : t.noActivity}
              </div>
            </div>
          ) : (
            sortedEntries.map((entry) => (
              <div key={entry.period} className="rollup-entry">
                <div className="rollup-entry-head">
                  <div className="slice-title-line">
                    <strong>{entry.label}</strong>
                    {entry.status && (
                      <span
                        className={`report-state-dot ${entry.status}`}
                        title={t.status(entry.status)}
                        aria-label={t.status(entry.status)}
                      />
                    )}
                  </div>
                  <div className="rollup-entry-actions">
                    {showHuman && notes.entryAddNote(entry.period)}
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs"
                      onClick={() => onJumpToPeriod(entry.period)}
                    >
                      {t.openReport(g)}
                    </button>
                  </div>
                </div>
                {showHuman && notes.entryTopNotes(entry.period)}
                {showAgent && entry.markdown.trim() !== "" && (
                  <MarkdownReport
                    markdown={entry.markdown}
                    onSelectAnchor={onSelectAnchor}
                    expandAll={expandCmd.expandAll}
                    expandSeq={expandCmd.seq}
                    pointExtras={showHuman ? notes.pointExtras(entry.period) : undefined}
                    pointAction={showHuman ? notes.pointAction(entry.period) : undefined}
                  />
                )}
                {showHuman && notes.entryNotes(entry.period, !showAgent)}
              </div>
            ))
          )}
        </div>
      </ReportCanvas>
      {marquee}
    </section>
  );
}
