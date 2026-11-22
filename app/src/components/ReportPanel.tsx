import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  MarkdownReport,
  type AnchorClickHandler,
  type OutlineExpandState,
} from "../anchors";
import type {
  GenStage,
  DailyReportState,
  ExportResult,
  PromptOption,
  ReportVersion,
} from "../types";
import { CopyButton } from "./CopyButton";
import { MoreMenu } from "./DropdownMenu";
import { EyeIcon, PencilIcon } from "./icons";
import { LabelSwap } from "./LabelSwap";
import { ExportModal } from "./ExportModal";
import { detailsToggle, useNoteFolds } from "../hooks/useDetailsToggle";
import { OutlineToggle } from "./OutlineToggle";
import { ReportCanvas } from "./ReportCanvas";
import { useLanguage } from "../i18n";
import { readItem, storeItem } from "../hooks/storage";
import { useHumanJournal } from "../hooks/useHumanJournal";
import { useNoteMarquee } from "../hooks/useNoteMarquee";
import { useHumanLayer } from "./human/HumanLayer";
import type { ProjectRef } from "./human/layout";
import { periodMarkdown } from "./human/journalMarkdown";
import { SourceFilterPills, useSourceFilter } from "./human/SourceFilter";

const SHOW_EDITS_KEY = "lantai.report.showEdits";

interface ReportPanelProps {
  /** The period: a date, a week key, or a month key. */
  date: string;
  kind?: "daily" | "weekly" | "monthly";
  scopeKey: string;
  workspaceSwitcher?: React.ReactNode;
  reportState: DailyReportState | null;
  prompts: PromptOption[];
  engines: string[];
  defaultEngine: string;
  selectedPrompt: string;
  selectedEngine: string;
  onSelectPrompt: (name: string) => void;
  onSelectEngine: (name: string) => void;
  generating: boolean;
  genStage: GenStage | null;
  /** `update` rewrites only the projects whose material changed since the current version. */
  onGenerate: (update?: boolean) => Promise<void>;
  onCancel: () => Promise<void>;
  onSaveEdit: (markdown: string) => Promise<void>;
  onSetStatus: (status: "draft" | "final") => Promise<void>;
  onRestoreVersion: (versionId: number) => Promise<void>;
  onExport: (params: {
    markdown: string;
    customPath: string | null;
    anchorMode: "keep" | "footnotes" | "strip";
    force: boolean;
  }) => Promise<ExportResult>;
  onSelectAnchor: AnchorClickHandler;
  /** Project names that have a project view; their sections get an "Open Project Report" link. */
  projectNames?: string[];
  onOpenProject?: (name: string) => void;
  onOpenWeek?: (week: string) => void;
  onOpenSessions?: () => void;
  /** All projects in the user's order, for the human journal. */
  projects: ProjectRef[];
  /** Open a day or a week linked from a note. */
  onOpenPeriod?: (key: string) => void;
  onError: (message: string) => void;
  /** After a note was added or removed (the sidebar counts them). */
  onNotesChanged?: () => void;
}

/** The project a `## <heading>` names (case-insensitive, ignoring `**`), if any. */
function matchProjectName(
  heading: string,
  names: string[] | undefined,
): string | null {
  const h = heading.replace(/\*\*/g, "").trim().toLowerCase();
  return names?.find((n) => n.toLowerCase() === h) ?? null;
}

export function ReportPanel({
  date,
  kind = "daily",
  scopeKey,
  workspaceSwitcher,
  reportState,
  prompts,
  engines,
  defaultEngine,
  selectedPrompt,
  selectedEngine,
  onSelectPrompt,
  onSelectEngine,
  generating,
  genStage,
  onGenerate,
  onCancel,
  onSaveEdit,
  onSetStatus,
  onRestoreVersion,
  onExport,
  onSelectAnchor,
  projectNames,
  onOpenProject,
  onOpenWeek,
  onOpenSessions,
  projects,
  onOpenPeriod,
  onError,
  onNotesChanged,
}: ReportPanelProps) {
  const { lang, t: tr } = useLanguage();
  const t = tr.report;
  // A version as listed in the history: number, origin, prompt and the language it is in.
  const versionLabel = (v: ReportVersion) =>
    `v${v.number} · ${t.origins[v.origin] ?? v.origin} · ${v.promptName ?? "-"} · ${v.language}`;
  const [mode, setMode] = useState<"preview" | "edit" | "compare">("preview");
  const [editorText, setEditorText] = useState("");
  const [exportOpen, setExportOpen] = useState(false);
  const [leftVersionId, setLeftVersionId] = useState<number | null>(null);
  const [rightVersionId, setRightVersionId] = useState<number | null>(null);
  const [snapshotOpen, setSnapshotOpen] = useState<string | null>(null);
  const [showEdits, setShowEdits] = useState(() => readItem(SHOW_EDITS_KEY) !== "0");
  const toggleShowEdits = () =>
    setShowEdits((prev) => {
      storeItem(SHOW_EDITS_KEY, prev ? "0" : "1");
      return !prev;
    });

  const [expandCmd, setExpandCmd] = useState<{
    expandAll: boolean;
    seq: number;
  }>({ expandAll: false, seq: 0 });
  const [outlineState, setOutlineState] = useState<OutlineExpandState>({
    hasNested: false,
    allCollapsed: true,
    allExpanded: false,
  });

  const handleOutlineStateChange = useCallback((next: OutlineExpandState) => {
    setOutlineState(next);
  }, []);

  const versions = useMemo(
    () => reportState?.versions ?? [],
    [reportState?.versions],
  );
  const currentVersion = useMemo(
    () =>
      versions.find((v) => v.isCurrent) ??
      versions[versions.length - 1] ??
      null,
    [versions],
  );

  useEffect(() => {
    setEditorText(currentVersion?.markdown ?? "");
  }, [currentVersion?.id, currentVersion?.markdown]);

  useEffect(() => {
    if (versions.length > 0) {
      setRightVersionId(versions[versions.length - 1].id);
      setLeftVersionId(
        versions.length > 1 ? versions[versions.length - 2].id : versions[0].id,
      );
    } else {
      setLeftVersionId(null);
      setRightVersionId(null);
    }
    setExportOpen(false);
    setMode("preview");
    setExpandCmd({ expandAll: false, seq: 0 });
  }, [scopeKey, versions.length]);

  const leftVersion = versions.find((v) => v.id === leftVersionId) ?? null;
  const rightVersion = versions.find((v) => v.id === rightVersionId) ?? null;
  const [sourceFilter, chooseSourceFilter] = useSourceFilter();
  const journal = useHumanJournal(
    { type: "period", kind, period: date },
    String(currentVersion?.id ?? "none"),
    onError,
    onNotesChanged,
  );
  const human = useHumanLayer({
    journal,
    projects,
    markdown: currentVersion?.markdown ?? "",
    links: { onSelectAnchor, onOpenPeriod },
  });
  const showHuman = sourceFilter !== "agent";
  // What the copy button copies: the text on screen under the current source filter.
  const copyText = () =>
    periodMarkdown({
      markdown: currentVersion?.markdown ?? "",
      journal,
      projects,
      filter: sourceFilter,
      labels: tr.human,
    });
  const copyButton = <CopyButton text={copyText} />;
  const marquee = useNoteMarquee(journal, showHuman);
  const noteFolds = useNoteFolds(journal, showHuman);
  const details = detailsToggle([
    {
      present: sourceFilter !== "human" && outlineState.hasNested,
      expanded: outlineState.allExpanded,
      setExpanded: (value) =>
        setExpandCmd((prev) => ({ expandAll: value, seq: prev.seq + 1 })),
    },
    noteFolds,
  ]);
  const detailsButton = details.visible && (
    <OutlineToggle expanded={details.expanded} onToggle={details.toggle} />
  );
  const currentStatus = reportState?.meta?.status ?? "draft";
  const edits = currentVersion?.edits ?? null;
  const editCount = edits
    ? edits.lines.length + edits.removed.reduce((n, r) => n + r.lines.length, 0)
    : 0;
  const estimate = reportState?.estimate;

  const staleness = currentVersion ? (estimate?.staleness ?? null) : null;
  const stalenessSummary = staleness
    ? [
        staleness.newSlices > 0 && t.newSlices(staleness.newSlices),
        staleness.changedSlices > 0 && t.changed(staleness.changedSlices),
        staleness.removedSlices > 0 && t.removed(staleness.removedSlices),
      ]
        .filter(Boolean)
        .join(tr.meta.listSeparator)
    : "";

  const estimateSummary = estimate
    ? (kind === "monthly"
        ? t.estimateMonthly(estimate.digestsCached, estimate.slicesUsed)
        : t.estimate(estimate.slicesUsed, estimate.digestsCached, estimate.digestsPending)) +
      (estimate.estimatedChars > 0 ? t.chars(estimate.estimatedChars.toLocaleString(lang)) : "")
    : "";
  const canGenerate =
    !!estimate &&
    (kind === "monthly" ? estimate.digestsCached > 0 : estimate.slicesUsed > 0);
  const unreportedWeeks =
    kind === "monthly" ? (estimate?.unreportedWeeks ?? []) : [];

  const unreportedBanner =
    unreportedWeeks.length > 0 ? (
      <div className="banner banner-info rollup-unreported">
        <span>{t.unreportedWeeks(date)}</span>
        <div className="rollup-date-pills">
          {unreportedWeeks.map((w) => (
            <button
              key={w}
              type="button"
              className="date-chip"
              onClick={() => onOpenWeek?.(w)}
              title={t.openWeek(w)}
            >
              {w}
            </button>
          ))}
        </div>
      </div>
    ) : null;

  const dotTitle = generating
    ? genStage === "stage2"
      ? t.writingStage
      : t.digestingStage
    : t.statusTitle(currentStatus, estimateSummary);

  // Draft / final and the version, or the generation stage; shown beside the report title.
  const statusBadge = generating ? (
    <span className="status-badge working" title={dotTitle}>
      <span className={`report-state-dot ${genStage ?? "stage1"}`} />
      {genStage === "stage2" ? t.writing : t.digesting}
    </span>
  ) : (
    currentVersion && (
      <button
        type="button"
        className={`status-badge ${currentStatus}`}
        title={t.toggleStatus(dotTitle, currentStatus === "final")}
        onClick={() => void onSetStatus(currentStatus === "final" ? "draft" : "final")}
      >
        <span className={`report-state-dot ${currentStatus}`} />
        <span>
          <LabelSwap
            current={currentStatus === "final" ? "final" : "draft"}
            labels={{ draft: t.draft, final: t.final }}
          />{" "}
          · v{currentVersion.number}
        </span>
      </button>
    )
  );

  return (
    <section className="panel report-panel">
      <header className="panel-header">
        <div className="topbar-left">{workspaceSwitcher}</div>

        <div className="topbar-right">
          <SourceFilterPills
            value={sourceFilter}
            onChange={chooseSourceFilter}
            disabled={mode !== "preview"}
          />

          <div className="pill-group icon-pills" role="radiogroup" aria-label={t.mode}>
            <button
              type="button"
              role="radio"
              className={`pill ${mode === "preview" ? "active" : ""}`}
              aria-checked={mode === "preview"}
              aria-label={t.preview}
              data-tip={t.preview}
              disabled={!currentVersion}
              onClick={() => setMode("preview")}
            >
              <EyeIcon />
            </button>
            <button
              type="button"
              role="radio"
              className={`pill ${mode === "edit" ? "active" : ""}`}
              aria-checked={mode === "edit"}
              aria-label={t.edit}
              data-tip={t.edit}
              disabled={!currentVersion}
              onClick={() => setMode("edit")}
            >
              <PencilIcon />
            </button>
          </div>

          {/* One button whose width fits its longest label, so nothing moves when it changes. */}
          <button
            type="button"
            className={`btn btn-sm ${generating ? "btn-danger" : "btn-primary"}`}
            disabled={!generating && !staleness && !canGenerate}
            title={
              staleness && !generating
                ? t.updateTitle(staleness.projects.join(tr.meta.listSeparator))
                : undefined
            }
            onClick={() =>
              generating
                ? void onCancel()
                : staleness
                  ? void onGenerate(true)
                  : void onGenerate()
            }
          >
            <LabelSwap
              current={generating ? "cancel" : staleness ? "update" : currentVersion ? "regenerate" : "generate"}
              labels={{
                generate: t.generate,
                regenerate: t.regenerate,
                update: t.update,
                cancel: tr.common.cancel,
              }}
            />
          </button>

          <MoreMenu tip={t.options}>
            {(close) => (
              <div className="dropdown-form">
                {estimateSummary && (
                  <div className="dropdown-meta">{estimateSummary}</div>
                )}
                <label className="dropdown-field">
                  <span>{t.prompt}</span>
                  <select
                    value={selectedPrompt}
                    onChange={(e) => onSelectPrompt(e.target.value)}
                  >
                    {prompts.map((p) => (
                      <option key={p.id} value={p.name}>
                        {p.name}
                        {p.isDefault ? t.defaultSuffix : ""}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="dropdown-field">
                  <span>{t.model}</span>
                  <select
                    value={selectedEngine || defaultEngine}
                    onChange={(e) => onSelectEngine(e.target.value)}
                  >
                    {engines.map((name) => (
                      <option key={name} value={name}>
                        {name}
                        {name === defaultEngine ? t.defaultSuffix : ""}
                      </option>
                    ))}
                  </select>
                </label>
                {currentVersion && (
                  <>
                    <div className="dropdown-sep" />
                    {staleness && !generating && (
                      <button
                        type="button"
                        className="dropdown-item"
                        onClick={() => {
                          void onGenerate();
                          close();
                        }}
                      >
                        {t.regenerateWhole}
                      </button>
                    )}
                    <button
                      type="button"
                      className="dropdown-item"
                      onClick={() => {
                        setMode("compare");
                        close();
                      }}
                    >
                      {t.versionHistory(versions.length)}
                    </button>
                    <button
                      type="button"
                      className="dropdown-item"
                      onClick={() => {
                        setExportOpen(true);
                        close();
                      }}
                    >
                      {t.exportMarkdown}
                    </button>
                  </>
                )}
              </div>
            )}
          </MoreMenu>
        </div>
      </header>

      <ExportModal
        open={exportOpen && currentVersion !== null}
        title={t.exportTitle(kind)}
        defaultExportPath={reportState?.defaultExportPath ?? `${date}.md`}
        markdown={
          mode === "edit" ? editorText : (currentVersion?.markdown ?? "")
        }
        onClose={() => setExportOpen(false)}
        onExport={onExport}
        onImportConflictAsVersion={onSaveEdit}
      />

      {!currentVersion ? (
        <ReportCanvas>
          <div className="report-document">
            {unreportedBanner}
            <div className="empty-card">
              <div className="empty-card-title">
                {t.noReport(kind, date)}
              </div>
              <div className="muted-line">
                {kind === "monthly" && estimate?.digestsCached === 0
                  ? t.weeklyFirst
                  : estimateSummary || t.reviewThen(kind)}
              </div>
              <div className="toolbar-actions">
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  disabled={generating || !canGenerate}
                  onClick={() => void onGenerate()}
                >
                  {generating ? t.generating : t.generateKind(kind)}
                </button>
                {onOpenSessions && (
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={onOpenSessions}
                  >
                    {t.reviewSessions}
                    {estimate ? ` (${estimate.slicesUsed})` : ""}
                  </button>
                )}
                {showHuman && human.addMenu}
                {showHuman && human.hasNotes && copyButton}
              </div>
            </div>
            {showHuman && human.humanOnly}
          </div>
        </ReportCanvas>
      ) : (
        <ReportCanvas>
          {mode === "preview" && (
            <div className="report-document">
              {unreportedBanner}
              {staleness && !generating && (
                <div className="banner banner-warn">
                  <span>
                    {t.outOfDate(stalenessSummary, staleness.projects.join(tr.meta.listSeparator))}
                  </span>
                  <div className="banner-actions">
                    <button
                      type="button"
                      className="btn btn-secondary btn-xs"
                      onClick={() => void onGenerate(true)}
                    >
                      {t.updateSections(staleness.projects.length)}
                    </button>
                  </div>
                </div>
              )}
              {sourceFilter === "human" ? (
                <div className="markdown-body">
                  <header className="report-doc-header">
                    <h1 className="report-doc-title">
                      {currentVersion.markdown.split("\n")[0].replace(/^#\s+/, "")}
                    </h1>
                    <div className="report-doc-actions">
                      <div className="report-doc-action-row">
                        {human.addMenu}
                        {statusBadge}
                        {copyButton}
                        {detailsButton}
                      </div>
                    </div>
                  </header>
                  {human.humanOnly}
                </div>
              ) : (
              <MarkdownReport
                markdown={currentVersion.markdown}
                onSelectAnchor={onSelectAnchor}
                edits={showEdits ? edits : null}
                beforeSections={showHuman ? human.dayNotes : undefined}
                afterSections={showHuman ? human.afterSections : undefined}
                pointExtras={showHuman ? human.pointExtras : undefined}
                pointAction={showHuman ? human.pointAction : undefined}
                sectionTop={showHuman ? human.sectionTop : undefined}
                sectionFooter={showHuman ? human.sectionFooter : undefined}
                sectionAction={(heading) => {
                  const name = matchProjectName(heading, projectNames);
                  return (
                    <>
                      {showHuman && human.sectionAddNote(heading)}
                      {name && onOpenProject && (
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs"
                          onClick={() => onOpenProject(name)}
                        >
                          {t.openProjectReport}
                        </button>
                      )}
                    </>
                  );
                }}
                headerAction={
                  <div className="report-doc-action-row">
                    {showHuman && human.addMenu}
                    {statusBadge}
                    {editCount > 0 && (
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs human-edits-toggle"
                        aria-pressed={showEdits}
                        title={tr.edits.toggleTitle}
                        onClick={toggleShowEdits}
                      >
                        {tr.edits.toggle(editCount)}
                      </button>
                    )}
                    {copyButton}
                    {detailsButton}
                  </div>
                }
                expandAll={expandCmd.expandAll}
                expandSeq={expandCmd.seq}
                onExpandStateChange={handleOutlineStateChange}
              />
              )}
            </div>
          )}

          {mode === "edit" && (
            <div className="report-document full-h">
              <div className="editor-wrap">
                <textarea
                  className="report-editor"
                  value={editorText}
                  onChange={(e) => setEditorText(e.target.value)}
                />
                <div className="editor-actions">
                  <button
                    type="button"
                    className="btn btn-primary btn-sm"
                    disabled={
                      editorText.trim() === currentVersion.markdown.trim()
                    }
                    onClick={async () => {
                      await onSaveEdit(editorText);
                      setMode("preview");
                    }}
                  >
                    {t.saveAsNewVersion}
                  </button>
                </div>
              </div>
            </div>
          )}

          {mode === "compare" && (
            <div className="report-document full-h">
              <div className="compare-wrap">
                <div className="compare-controls">
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs"
                    onClick={() => setMode("preview")}
                  >
                    {t.back}
                  </button>
                  <label>
                    {t.left}
                    <select
                      value={leftVersionId ?? ""}
                      onChange={(e) => setLeftVersionId(Number(e.target.value))}
                    >
                      {versions.map((v) => (
                        <option key={v.id} value={v.id}>
                          {versionLabel(v)}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    {t.right}
                    <select
                      value={rightVersionId ?? ""}
                      onChange={(e) =>
                        setRightVersionId(Number(e.target.value))
                      }
                    >
                      {versions.map((v) => (
                        <option key={v.id} value={v.id}>
                          {versionLabel(v)}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <div className="compare-grid">
                  {[leftVersion, rightVersion].map((v, idx) =>
                    v ? (
                      <div key={`${v.id}-${idx}`} className="compare-col">
                        <div className="compare-col-head">
                          <span>
                            {versionLabel(v)} · {v.engine ?? t.manual}
                          </span>
                          <div className="toolbar-actions">
                            {v.promptSnapshot && (
                              <button
                                type="button"
                                className="btn btn-ghost btn-xs"
                                onClick={() =>
                                  setSnapshotOpen(v.promptSnapshot)
                                }
                              >
                                {t.prompt}
                              </button>
                            )}
                            {!v.isCurrent && (
                              <button
                                type="button"
                                className="btn btn-secondary btn-xs"
                                onClick={() => void onRestoreVersion(v.id)}
                              >
                                {t.setCurrent}
                              </button>
                            )}
                          </div>
                        </div>
                        <pre className="compare-text">{v.markdown}</pre>
                      </div>
                    ) : null,
                  )}
                </div>
                {snapshotOpen && (
                  <div className="snapshot-box">
                    <div className="compare-col-head">
                      <span>{t.promptSnapshot}</span>
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => setSnapshotOpen(null)}
                      >
                        {tr.common.close}
                      </button>
                    </div>
                    <pre className="compare-text">{snapshotOpen}</pre>
                  </div>
                )}
              </div>
            </div>
          )}
        </ReportCanvas>
      )}
      {marquee}
    </section>
  );
}
