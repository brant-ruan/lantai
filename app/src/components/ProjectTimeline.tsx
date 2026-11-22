import React, { useEffect, useMemo, useState } from "react";
import { AgentBadge } from "./AgentBadge";
import type { ProjectRangeKey, ProjectWorkbench, SliceCard } from "../types";
import { BatchBar } from "./BatchBar";
import { ArchivedBanner, ProjectMenu, type ProjectMenuActions } from "./ProjectMenu";
import { RangeSelect } from "./RangeSelect";
import type { ProjectManage } from "../hooks/useProjectView";
import { InfoTip } from "./InfoTip";
import { SliceRow } from "./SliceRow";
import { MarqueeRect } from "./MarqueeRect";
import { useSliceSelection } from "../hooks/useSliceSelection";
import { useT } from "../i18n";

/** Suggested sessions listed before "Show all". */
const SUGGESTIONS_SHOWN = 3;

interface ProjectTimelineProps {
  workbench: ProjectWorkbench;
  /** The management card open above the sessions, if any. */
  manage: ProjectManage;
  menu: ProjectMenuActions;
  workspaceSwitcher?: React.ReactNode;
  range: ProjectRangeKey;
  onChangeRange: (range: ProjectRangeKey) => void;
  selectedSliceId: number | null;
  onOpenSlice: (sliceId: number) => void;
  onRenameAndDescribe: (
    projectId: number,
    newName: string,
    description: string | null,
  ) => Promise<void>;
  onAcceptSuggestion: (sessionIds: number[]) => Promise<void>;
  onBatchAssign: (
    sessionIds: number[],
    projectId: number | null,
  ) => Promise<void>;
  onBatchUnassign: (sessionIds: number[]) => Promise<void>;
  onBatchSetIgnoreState: (
    sliceIds: number[],
    state: "ignored" | "included",
  ) => Promise<void>;
  onAddRule: (params: {
    projectId: number;
    ruleType: string;
    targetField: string | null;
    pattern: string;
    autoApply: boolean;
  }) => Promise<void>;
  onDeleteRule: (id: number) => Promise<void>;
  onSetRuleAuto: (id: number, autoApply: boolean) => Promise<void>;
}

export function ProjectTimeline({
  workbench,
  manage,
  menu,
  workspaceSwitcher,
  range,
  onChangeRange,
  selectedSliceId,
  onOpenSlice,
  onRenameAndDescribe,
  onAcceptSuggestion,
  onBatchAssign,
  onBatchUnassign,
  onBatchSetIgnoreState,
  onAddRule,
  onDeleteRule,
  onSetRuleAuto,
}: ProjectTimelineProps) {
  const tr = useT();
  const t = tr.timeline;
  const ps = tr.projectsSettings;
  const { project, slices, suggestedSlices, rules, projects } = workbench;
  const [showIgnored, setShowIgnored] = useState(false);
  const [showAllSuggestions, setShowAllSuggestions] = useState(false);

  const [editName, setEditName] = useState(project.name);
  const [editDesc, setEditDesc] = useState(project.description ?? "");
  useEffect(() => {
    if (manage === "details") {
      setEditName(project.name);
      setEditDesc(project.description ?? "");
    }
  }, [manage, project.name, project.description]);

  const [ruleType, setRuleType] = useState("path_prefix");
  const [ruleField, setRuleField] = useState("any");
  const [rulePattern, setRulePattern] = useState("");
  const [ruleAuto, setRuleAuto] = useState(false);

  const activeSlices = useMemo(
    () => slices.filter((s) => !s.ignored),
    [slices],
  );
  const ignoredSlices = useMemo(
    () => slices.filter((s) => s.ignored),
    [slices],
  );

  const groupedByDate = useMemo(() => {
    const map = new Map<string, SliceCard[]>();
    for (const s of activeSlices) {
      const list = map.get(s.sliceDate) ?? [];
      list.push(s);
      map.set(s.sliceDate, list);
    }
    return [...map.entries()];
  }, [activeSlices]);

  const suggestionSessionIds = useMemo(
    () => [...new Set(suggestedSlices.map((s) => s.sessionId))],
    [suggestedSlices],
  );

  // Rows in display order, for range selection and "Select all".
  const visibleSliceIds = useMemo(() => {
    const ids = groupedByDate.flatMap(([, items]) => items.map((s) => s.id));
    if (showIgnored) {
      for (const s of ignoredSlices) ids.push(s.id);
    }
    return ids;
  }, [groupedByDate, ignoredSlices, showIgnored]);

  const selection = useSliceSelection(visibleSliceIds, project.id);
  const selecting = selection.selected.length > 0;
  const selectedSet = useMemo(
    () => new Set(selection.selected),
    [selection.selected],
  );

  const rowProps = (slice: SliceCard, showDate: boolean) => ({
    slice,
    showDate,
    opened: selectedSliceId === slice.id,
    checked: selectedSet.has(slice.id),
    selecting,
    onToggleChecked: () => selection.toggle(slice.id),
    onClick: (e: React.MouseEvent) => {
      if (!selection.onRowClick(e, slice.id)) onOpenSlice(slice.id);
    },
  });

  const handleAddRule = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!rulePattern.trim()) return;
    await onAddRule({
      projectId: project.id,
      ruleType,
      targetField: ruleType === "regex" ? ruleField : null,
      pattern: rulePattern.trim(),
      autoApply: ruleAuto,
    });
    setRulePattern("");
  };

  return (
    <section className="panel slice-panel">
      <header className="panel-header">
        <div className="topbar-left">{workspaceSwitcher}</div>

        <div className="topbar-right">
          {selecting ? (
            <BatchBar
              slices={slices}
              visibleSliceIds={visibleSliceIds}
              selectedSliceIds={selection.selected}
              projects={projects}
              onChangeSelected={selection.setSelected}
              onBatchAssign={onBatchAssign}
              onBatchUnassign={onBatchUnassign}
              onBatchSetIgnoreState={onBatchSetIgnoreState}
              onClose={selection.clear}
            />
          ) : (
            <>
              <RangeSelect value={range} onChange={onChangeRange} />
              <ProjectMenu actions={menu}>
                {(close) =>
                  ignoredSlices.length > 0 && (
                    <>
                      <button
                        type="button"
                        className="dropdown-item"
                        onClick={() => {
                          setShowIgnored((v) => !v);
                          close();
                        }}
                      >
                        {t.toggleIgnored(showIgnored, ignoredSlices.length)}
                      </button>
                      <div className="dropdown-sep" />
                    </>
                  )
                }
              </ProjectMenu>
            </>
          )}
        </div>
      </header>

      <div
        className="workspace-scroll marquee-host"
        ref={selection.scrollRef}
        onMouseDown={selection.onMouseDown}
      >
        <div className="workspace-container">
          <ArchivedBanner actions={menu} />
          {manage === "details" && (
            <form
              className="settings-card stack"
              onSubmit={async (e) => {
                e.preventDefault();
                if (!editName.trim()) return;
                await onRenameAndDescribe(
                  project.id,
                  editName.trim(),
                  editDesc.trim() || null,
                );
                menu.onManage(null);
              }}
            >
              <div className="form-grid-2">
                <label className="form-field">
                  <span>{ps.projectName}</span>
                  <input
                    type="text"
                    value={editName}
                    onChange={(e) => setEditName(e.target.value)}
                    required
                  />
                </label>
                <label className="form-field">
                  <span className="title-with-tip">
                    {ps.description}
                    <InfoTip>{ps.descriptionTip}</InfoTip>
                  </span>
                  <input
                    type="text"
                    placeholder={ps.optional}
                    value={editDesc}
                    onChange={(e) => setEditDesc(e.target.value)}
                  />
                </label>
              </div>
              <div className="settings-card-foot form-foot-end">
                <div className="toolbar-actions">
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    onClick={() => menu.onManage(null)}
                  >
                    {tr.common.cancel}
                  </button>
                  <button type="submit" className="btn btn-primary btn-sm">
                    {tr.common.save}
                  </button>
                </div>
              </div>
            </form>
          )}

          {manage === "rules" && (
            <div className="settings-stack">
              <form
                className="settings-card stack"
                onSubmit={(e) => void handleAddRule(e)}
              >
                <div className="settings-card-head">
                  <h3>{t.addRuleFor(project.name)}</h3>
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs"
                    onClick={() => menu.onManage(null)}
                  >
                    {tr.common.close}
                  </button>
                </div>
                <div className="rule-builder-row">
                  <select
                    value={ruleType}
                    onChange={(e) => setRuleType(e.target.value)}
                  >
                    <option value="path_prefix">{ps.ruleTypes.path_prefix}</option>
                    <option value="git_remote">{ps.ruleTypes.git_remote}</option>
                    <option value="regex">{ps.ruleTypes.regex}</option>
                  </select>
                  {ruleType === "regex" && (
                    <select
                      value={ruleField}
                      onChange={(e) => setRuleField(e.target.value)}
                    >
                      <option value="any">{ps.fields.any}</option>
                      <option value="title">{ps.fields.title}</option>
                      <option value="first_prompt">{ps.fields.first_prompt}</option>
                      <option value="cwd">{ps.fields.cwd}</option>
                    </select>
                  )}
                  <input
                    type="text"
                    className="flex-1"
                    placeholder={
                      ruleType === "path_prefix"
                        ? "/abs/path/to/repo"
                        : ruleType === "git_remote"
                          ? "github.com/owner/repo"
                          : ps.regexPlaceholder
                    }
                    value={rulePattern}
                    onChange={(e) => setRulePattern(e.target.value)}
                    required
                  />
                </div>
                <div className="settings-card-foot">
                  <label className="toggle-row">
                    <input
                      type="checkbox"
                      checked={ruleAuto}
                      onChange={(e) => setRuleAuto(e.target.checked)}
                    />
                    <span>{ps.autoOnMatch}</span>
                  </label>
                  <button type="submit" className="btn btn-primary btn-sm">
                    {ps.addRule}
                  </button>
                </div>
              </form>

              <div className="settings-card">
                <div className="settings-card-head">
                  <h3>{ps.activeRules(rules.length)}</h3>
                </div>
                {rules.length === 0 ? (
                  <div className="empty-state">
                    {t.noRulesFor(project.name)}
                  </div>
                ) : (
                  <div className="settings-list">
                    {rules.map((r) => (
                      <div key={r.id} className="settings-item-row">
                        <div className="settings-item-main">
                          <div className="slice-title-line">
                            <span className="badge">
                              {ps.ruleTypes[r.ruleType] ?? r.ruleType}
                              {r.targetField ? `(${r.targetField})` : ""}
                            </span>
                            <code className="rule-pattern-code">
                              {r.pattern}
                            </code>
                          </div>
                        </div>
                        <div className="settings-item-actions">
                          <label className="toggle-row">
                            <input
                              type="checkbox"
                              checked={r.autoApply}
                              onChange={(e) =>
                                void onSetRuleAuto(r.id, e.target.checked)
                              }
                            />
                            <span>{ps.auto}</span>
                          </label>
                          <button
                            type="button"
                            className="btn btn-ghost-danger btn-xs"
                            onClick={() => void onDeleteRule(r.id)}
                          >
                            {tr.common.delete}
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          {suggestedSlices.length > 0 && (
              <div className="settings-card">
                <div className="settings-card-head">
                  <h3>{t.suggested(suggestedSlices.length)}</h3>
                  <button
                    type="button"
                    className="btn btn-primary btn-xs"
                    onClick={() => void onAcceptSuggestion(suggestionSessionIds)}
                  >
                    {t.acceptAll(suggestionSessionIds.length)}
                  </button>
                </div>
                <div className="settings-list">
                  {(showAllSuggestions
                    ? suggestedSlices
                    : suggestedSlices.slice(0, SUGGESTIONS_SHOWN)
                  ).map((slice) => (
                    <div key={slice.id} className="settings-item-row">
                      <button
                        type="button"
                        className="slice-main"
                        onClick={() => onOpenSlice(slice.id)}
                      >
                        <div className="slice-title-line">
                          <span className="slice-title">{slice.title}</span>
                        </div>
                        <div className="slice-meta-line">
                          <AgentBadge
                            agent={slice.agent}
                            name={slice.agentName}
                          />
                          <span>{slice.sliceDate}</span>
                          <span>
                            {slice.startTime}-{slice.endTime}
                          </span>
                          <span>{slice.activeMinutes}m</span>
                          <span>
                            {tr.sliceList.suggestion(
                              slice.suggestionSource,
                              slice.suggestionReason,
                            )}
                          </span>
                        </div>
                      </button>
                      <div className="settings-item-actions">
                        <button
                          type="button"
                          className="btn btn-secondary btn-xs"
                          onClick={() =>
                            void onAcceptSuggestion([slice.sessionId])
                          }
                        >
                          {t.accept}
                        </button>
                      </div>
                    </div>
                  ))}
                  {suggestedSlices.length > SUGGESTIONS_SHOWN && (
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs suggestions-more"
                      onClick={() => setShowAllSuggestions((v) => !v)}
                    >
                      {showAllSuggestions ? t.showFewer : t.showAll(suggestedSlices.length)}
                    </button>
                  )}
                </div>
              </div>
          )}

          {(
            <div className="slice-groups">
              {groupedByDate.length === 0 ? (
                <div className="empty-card">
                  <div className="empty-card-title">
                    {t.emptyTitle}
                  </div>
                  <div className="muted-line">
                    {t.empty(project.name)}
                  </div>
                </div>
              ) : (
                groupedByDate.map(([dateKey, items]) => (
                  <div key={dateKey} className="slice-group">
                    <div className="group-heading">
                      <span>{dateKey}</span>
                      <span className="muted-line">{items.length}</span>
                    </div>
                    {items.map((slice) => (
                      <SliceRow key={slice.id} {...rowProps(slice, false)}>
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs slice-hover-action"
                          onClick={() =>
                            void onBatchSetIgnoreState([slice.id], "ignored")
                          }
                          title={t.ignoreTitle}
                        >
                          {tr.sliceList.ignore}
                        </button>
                      </SliceRow>
                    ))}
                  </div>
                ))
              )}

              {showIgnored &&
                ignoredSlices.map((slice) => (
                  <SliceRow key={slice.id} {...rowProps(slice, true)}>
                    <button
                      type="button"
                      className="btn btn-secondary btn-xs"
                      onClick={() =>
                        void onBatchSetIgnoreState([slice.id], "included")
                      }
                    >
                      {tr.sliceList.include}
                    </button>
                  </SliceRow>
                ))}
            </div>
          )}

        </div>
        <MarqueeRect rect={selection.marquee} />
      </div>
    </section>
  );
}
