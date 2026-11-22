// Sessions tab of a day, week or month: slices grouped by project, each with its project
// picker and a hover Ignore button. Rows can be multi-selected (drag a rectangle, or
// Cmd/Shift-click) for batch actions; Escape leaves the selection.

import React, { useEffect, useMemo, useRef, useState } from "react";
import type { ProjectOption, SliceCard } from "../types";
import { BatchBar } from "./BatchBar";
import { SliceRow } from "./SliceRow";
import { MarqueeRect } from "./MarqueeRect";
import { useSliceSelection } from "../hooks/useSliceSelection";
import { useT } from "../i18n";
import { assignableProjects } from "../projectOptions";

interface SliceListProps {
  date: string;
  /** Show each slice's date (when the list spans several days). */
  showDate?: boolean;
  workspaceSwitcher?: React.ReactNode;
  slices: SliceCard[];
  projects: ProjectOption[];
  selectedSliceId: number | null;
  onOpenSlice: (sliceId: number) => void;
  onAssignSession: (
    sessionId: number,
    projectId: number | null,
  ) => Promise<void>;
  onAssignSlice: (sliceId: number, projectId: number | null) => Promise<void>;
  onUnassignSession: (sessionId: number) => Promise<void>;
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
  /** Create a project and, when `slice` is given, move that slice to it. */
  onQuickCreateProject: (
    name: string,
    description: string | null,
    slice: SliceCard | null,
  ) => Promise<void>;
  ruleHint: { projectId: number; projectName: string; path: string } | null;
  onCreatePathRule: (
    projectId: number,
    path: string,
    autoApply: boolean,
  ) => Promise<void>;
  onDismissRuleHint: () => void;
  onToggleSliceIgnore: (slice: SliceCard) => Promise<void>;
}

type Filter = "all" | "unassigned" | "ignored";

const NEW_PROJECT_VALUE = "__new__";
const NONE_VALUE = "__none__";
const UNASSIGN_VALUE = "__unassign__";
const FOLLOW_SESSION_VALUE = "__follow__";

/** Group keys of slices without a project; real project names never start with NUL. */
const UNASSIGNED_GROUP = "\u0000unassigned";
const NO_PROJECT_GROUP = "\u0000none";

const isUnassigned = (s: SliceCard) =>
  s.projectId === null && s.assignmentSource !== "manual";

export function SliceList({
  date,
  showDate = false,
  workspaceSwitcher,
  slices,
  projects,
  selectedSliceId,
  onOpenSlice,
  onAssignSession,
  onAssignSlice,
  onUnassignSession,
  onAcceptSuggestion,
  onBatchAssign,
  onBatchUnassign,
  onBatchSetIgnoreState,
  onQuickCreateProject,
  ruleHint,
  onCreatePathRule,
  onDismissRuleHint,
  onToggleSliceIgnore,
}: SliceListProps) {
  const tr = useT();
  const t = tr.sliceList;
  const [filter, setFilter] = useState<Filter>("all");
  // The new-project form; `forSlice` is the row whose picker opened it.
  const [newProject, setNewProject] = useState<{
    forSlice: SliceCard | null;
  } | null>(null);
  const [newProjName, setNewProjName] = useState("");
  const [newProjDesc, setNewProjDesc] = useState("");
  const newProjectRef = useRef<HTMLFormElement | null>(null);

  const activeSlices = useMemo(
    () => slices.filter((s) => !s.ignored),
    [slices],
  );
  const ignoredSlices = useMemo(
    () => slices.filter((s) => s.ignored),
    [slices],
  );
  const unassignedCount = useMemo(
    () => activeSlices.filter(isUnassigned).length,
    [activeSlices],
  );
  const suggestionSessionIds = useMemo(() => {
    const ids = new Set<number>();
    for (const s of activeSlices) {
      if (s.projectId === null && s.suggestedProjectId !== null)
        ids.add(s.sessionId);
    }
    return [...ids];
  }, [activeSlices]);

  // The ignored filter disappears with the last ignored slice.
  const shownFilter: Filter =
    filter === "ignored" && ignoredSlices.length === 0 ? "all" : filter;

  const grouped = useMemo(() => {
    if (shownFilter === "ignored") return [];
    const visible =
      shownFilter === "unassigned"
        ? activeSlices.filter(isUnassigned)
        : activeSlices;
    const map = new Map<string, SliceCard[]>();
    for (const s of visible) {
      const label =
        s.projectName ??
        (s.assignmentSource === "manual" ? NO_PROJECT_GROUP : UNASSIGNED_GROUP);
      const list = map.get(label) ?? [];
      list.push(s);
      map.set(label, list);
    }
    const entries = [...map.entries()];
    entries.sort(([a], [b]) => {
      if (a === UNASSIGNED_GROUP) return -1;
      if (b === UNASSIGNED_GROUP) return 1;
      if (a === NO_PROJECT_GROUP) return 1;
      if (b === NO_PROJECT_GROUP) return -1;
      return a.localeCompare(b);
    });
    return entries;
  }, [activeSlices, shownFilter]);

  // Rows in display order, for range selection and "Select all".
  const visibleSliceIds = useMemo(
    () =>
      shownFilter === "ignored"
        ? ignoredSlices.map((s) => s.id)
        : grouped.flatMap(([, items]) => items.map((s) => s.id)),
    [grouped, ignoredSlices, shownFilter],
  );

  const selection = useSliceSelection(visibleSliceIds, date);
  const selecting = selection.selected.length > 0;
  const selectedSet = useMemo(
    () => new Set(selection.selected),
    [selection.selected],
  );

  useEffect(() => {
    if (newProject) newProjectRef.current?.scrollIntoView({ block: "nearest" });
  }, [newProject]);

  const handleAssignSelect = async (slice: SliceCard, val: string) => {
    if (val === NEW_PROJECT_VALUE) {
      setNewProject({ forSlice: slice });
      return;
    }
    const hasOverride = slice.projectOverrideId !== null;
    if (val === FOLLOW_SESSION_VALUE) {
      await onAssignSlice(slice.id, null);
      return;
    }
    if (val === UNASSIGN_VALUE || val === NONE_VALUE) {
      // A day override cannot express "no project" (null means "follow the
      // session"), so drop the override and apply the choice to the session.
      if (hasOverride) await onAssignSlice(slice.id, null);
      if (val === UNASSIGN_VALUE) {
        await onUnassignSession(slice.sessionId);
      } else {
        await onAssignSession(slice.sessionId, null);
      }
      return;
    }
    const projectId = Number(val);
    if (hasOverride) {
      await onAssignSlice(slice.id, projectId);
    } else {
      await onAssignSession(slice.sessionId, projectId);
    }
  };

  const closeNewProject = () => {
    setNewProject(null);
    setNewProjName("");
    setNewProjDesc("");
  };

  const handleQuickCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newProject || !newProjName.trim()) return;
    await onQuickCreateProject(
      newProjName.trim(),
      newProjDesc.trim() || null,
      newProject.forSlice,
    );
    closeNewProject();
  };

  const rowProps = (slice: SliceCard) => ({
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

  const filterPill = (key: Filter, label: string, disabled = false) => (
    <button
      type="button"
      className={`pill ${shownFilter === key ? "active" : ""}`}
      disabled={disabled}
      onClick={() => setFilter(key)}
    >
      {label}
    </button>
  );

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
              <div className="pill-group">
                {filterPill("all", t.all(activeSlices.length))}
                {filterPill("unassigned", t.unassignedFilter(unassignedCount))}
                {filterPill(
                  "ignored",
                  t.ignoredFilter(ignoredSlices.length),
                  ignoredSlices.length === 0,
                )}
              </div>
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
          {newProject && (
            <form
              ref={newProjectRef}
              className="inline-card"
              onSubmit={(e) => void handleQuickCreate(e)}
            >
              <input
                type="text"
                placeholder={
                  newProject.forSlice
                    ? t.newProjectFor(newProject.forSlice.title)
                    : t.projectNamePlaceholder
                }
                value={newProjName}
                onChange={(e) => setNewProjName(e.target.value)}
                autoFocus
                required
              />
              <input
                type="text"
                placeholder={t.descriptionPlaceholder}
                value={newProjDesc}
                onChange={(e) => setNewProjDesc(e.target.value)}
              />
              <button type="submit" className="btn btn-primary btn-sm">
                {tr.common.create}
              </button>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={closeNewProject}
              >
                {tr.common.cancel}
              </button>
            </form>
          )}

          {suggestionSessionIds.length > 0 && (
            <div className="banner banner-info">
              <span>{t.suggestionsBanner(suggestionSessionIds.length)}</span>
              <div className="banner-actions">
                <button
                  type="button"
                  className="btn btn-primary btn-xs"
                  onClick={() => void onAcceptSuggestion(suggestionSessionIds)}
                >
                  {t.acceptSuggestions(suggestionSessionIds.length)}
                </button>
              </div>
            </div>
          )}

          {ruleHint && (
            <div className="banner banner-info">
              <span>
                {t.noRuleBefore}
                <code>{ruleHint.path}</code>
                {t.noRuleMiddle}
                <strong>{ruleHint.projectName}</strong>
                {t.noRuleAfter}
              </span>
              <div className="banner-actions">
                <button
                  type="button"
                  className="btn btn-secondary btn-xs"
                  onClick={() =>
                    void onCreatePathRule(
                      ruleHint.projectId,
                      ruleHint.path,
                      false,
                    )
                  }
                >
                  {t.suggestOnly}
                </button>
                <button
                  type="button"
                  className="btn btn-primary btn-xs"
                  onClick={() =>
                    void onCreatePathRule(
                      ruleHint.projectId,
                      ruleHint.path,
                      true,
                    )
                  }
                >
                  {t.autoAssign}
                </button>
                <button
                  type="button"
                  className="btn btn-ghost btn-xs"
                  onClick={onDismissRuleHint}
                >
                  {t.dismiss}
                </button>
              </div>
            </div>
          )}

          <div className="slice-groups">
            {shownFilter === "ignored" ? (
              <div className="slice-group">
                {ignoredSlices.map((slice) => (
                  <SliceRow key={slice.id} {...rowProps(slice)}>
                    <button
                      type="button"
                      className="btn btn-secondary btn-xs"
                      onClick={() => void onToggleSliceIgnore(slice)}
                    >
                      {t.include}
                    </button>
                  </SliceRow>
                ))}
              </div>
            ) : grouped.length === 0 ? (
              <div className="empty-state">
                {t.empty(date)}
              </div>
            ) : (
              grouped.map(([groupName, items]) => (
                <div key={groupName} className="slice-group">
                  <div className="group-heading">
                    <span>
                      {groupName === UNASSIGNED_GROUP
                        ? t.unassigned
                        : groupName === NO_PROJECT_GROUP
                          ? t.noProject
                          : groupName}
                    </span>
                    <span className="muted-line">{items.length}</span>
                  </div>
                  {items.map((slice) => {
                    const selectVal =
                      slice.projectId !== null
                        ? String(slice.projectId)
                        : slice.assignmentSource === "manual"
                          ? NONE_VALUE
                          : UNASSIGN_VALUE;
                    return (
                      <SliceRow key={slice.id} {...rowProps(slice)}>
                        <button
                          type="button"
                          className="btn btn-ghost btn-xs slice-hover-action"
                          onClick={() => void onToggleSliceIgnore(slice)}
                          title={t.ignoreTitle}
                        >
                          {t.ignore}
                        </button>
                        {slice.projectId === null &&
                          slice.suggestedProjectId !== null && (
                            <button
                              type="button"
                              className="btn btn-secondary btn-xs"
                              title={
                                slice.suggestionReason
                                  ? t.suggestion(slice.suggestionSource, slice.suggestionReason)
                                  : undefined
                              }
                              onClick={() =>
                                void onAcceptSuggestion([slice.sessionId])
                              }
                            >
                              ✓ {slice.suggestedProjectName}
                            </button>
                          )}
                        <select
                          value={selectVal}
                          onChange={(e) =>
                            void handleAssignSelect(slice, e.target.value)
                          }
                          title={t.assignTitle}
                        >
                          <option value={NEW_PROJECT_VALUE}>
                            {t.newProject}
                          </option>
                          {slice.projectOverrideId !== null && (
                            <option value={FOLLOW_SESSION_VALUE}>
                              {t.followSession}
                            </option>
                          )}
                          <option value={UNASSIGN_VALUE}>{t.unassigned}</option>
                          {assignableProjects(projects, slice.projectId).map((p) => (
                            <option key={p.id} value={p.id}>
                              {p.name}
                            </option>
                          ))}
                          <option value={NONE_VALUE}>{t.noProject}</option>
                        </select>
                      </SliceRow>
                    );
                  })}
                </div>
              ))
            )}
          </div>
        </div>
        <MarqueeRect rect={selection.marquee} />
      </div>
    </section>
  );
}
