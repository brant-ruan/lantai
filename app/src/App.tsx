// Application shell: wires the sidebar, the active workspace view, the slice drawer and the
// settings modal together. View state and refresh logic live in hooks under `hooks/`.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { ProjectRollupPanel } from "./components/ProjectRollupPanel";
import { ProjectTimeline } from "./components/ProjectTimeline";
import { ReportPanel } from "./components/ReportPanel";
import { useResizableValue } from "./components/Resizable";
import { resetReportWidth } from "./hooks/useReportWidth";
import { SettingsModal } from "./components/SettingsModal";
import { Sidebar } from "./components/Sidebar";
import { weekMonth } from "./components/sidebar/dates";
import { SliceDrawer } from "./components/SliceDrawer";
import { SliceList } from "./components/SliceList";
import { StatsView } from "./components/StatsView";
import { SearchView } from "./components/SearchView";
import { WorkspaceTabs } from "./components/WorkspaceTabs";
import { useAppearance } from "./hooks/useAppearance";
import { useDateRollover } from "./hooks/useDateRollover";
import { useReportBatch } from "./hooks/useReportBatch";
import { useMenuActions } from "./hooks/useMenuActions";
import { useSearchView } from "./hooks/useSearchView";
import { usePeriodView, type PeriodLoaders, type PeriodView } from "./hooks/usePeriodView";
import { usePreDigest } from "./hooks/usePreDigest";
import { useProjectView } from "./hooks/useProjectView";
import { useReportJobs } from "./hooks/useReportJobs";
import { useSliceDrawer } from "./hooks/useSliceDrawer";
import { useStatsView } from "./hooks/useStatsView";
import { readItem, storeItem } from "./hooks/storage";
import { useUpdateCheck } from "./hooks/useUpdateCheck";
import { UpdateBanner } from "./components/UpdateBanner";
import { ErrorBanner } from "./components/FailureNote";
import type {
  DayOverview,
  DayWorkbench,
  Diagnostics,
  MonthOverview,
  PeriodMode,
  ProjectsOverview,
  PromptItem,
  SettingsTab,
  SliceCard,
  ViewMode,
  WeekOverview,
} from "./types";
import { useLanguage } from "./i18n";
import { DeleteProjectDialog, type DeleteTarget } from "./components/DeleteProjectDialog";

/** Fixed sidebar width: wide enough for the full calendar, not resizable. */
const SIDEBAR_WIDTH = 240;

const DAILY_LOADERS: PeriodLoaders = {
  workbench: api.dayWorkbench,
  reportState: (date, engine) => api.getDailyReportState(date, null, engine),
};
const WEEKLY_LOADERS: PeriodLoaders = {
  workbench: api.weekWorkbench,
  reportState: api.getWeeklyReportState,
};
const MONTHLY_LOADERS: PeriodLoaders = {
  workbench: api.monthWorkbench,
  reportState: api.getMonthlyReportState,
};

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function activeSliceCount(slices: { ignored: boolean }[] | undefined): number {
  return slices ? slices.filter((s) => !s.ignored).length : 0;
}

/** Load a period view whenever it is shown or its selection changes. */
function useLoadWhenShown(
  shown: boolean,
  view: PeriodView,
  setRuleHint: (hint: null) => void,
  runAction: (fn: () => Promise<void>) => Promise<void>,
) {
  const { selected, refresh } = view;
  useEffect(() => {
    if (shown && selected) {
      setRuleHint(null);
      void runAction(() => refresh(selected));
    }
  }, [shown, selected, refresh, setRuleHint, runAction]);
}

export function App() {
  const { t: tr, reportLanguage } = useLanguage();
  const [viewMode, setViewMode] = useState<ViewMode>("daily");
  const viewModeRef = useRef(viewMode);
  viewModeRef.current = viewMode;
  // The view to go back to when Search is left with Escape.
  const beforeSearchRef = useRef<ViewMode>("daily");
  if (viewMode !== "search") beforeSearchRef.current = viewMode;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("appearance");
  const closeSettings = useCallback(() => setSettingsOpen(false), []);
  const search = useSearchView();
  const updates = useUpdateCheck();
  useMenuActions({
    setViewMode: (mode) => {
      setViewMode(mode);
      // Cmd+F also moves focus to the search box when the view is already open.
      if (mode === "search") search.focus();
    },
    openSettings: (tab) => {
      if (tab) setSettingsTab(tab);
      setSettingsOpen(true);
    },
    closeSettings,
    checkUpdates: () => void updates.checkNow(true),
  });
  const appearance = useAppearance();
  const drawerSize = useResizableValue("aj-drawer-width", 540, 360, 820);

  const [days, setDays] = useState<DayOverview[]>([]);
  const [today, setToday] = useState("");
  const [showIgnoredDays, setShowIgnoredDays] = useState(
    () => readItem("aj-show-ignored-days") === "1",
  );
  const showIgnoredDaysRef = useRef(showIgnoredDays);
  showIgnoredDaysRef.current = showIgnoredDays;
  const [weeks, setWeeks] = useState<WeekOverview[]>([]);
  const [currentWeek, setCurrentWeek] = useState("");
  const [months, setMonths] = useState<MonthOverview[]>([]);
  const [currentMonth, setCurrentMonth] = useState("");
  const [prompts, setPrompts] = useState<PromptItem[]>([]);
  const [projectsOverview, setProjectsOverview] = useState<ProjectsOverview | null>(null);
  const [health, setHealth] = useState<Diagnostics | null>(null);

  // The engine is shared by all report views; refreshes read it through the ref so that
  // changing it does not recreate their callbacks (which would fetch twice).
  const [selectedEngine, setSelectedEngine] = useState("");
  const selectedEngineRef = useRef(selectedEngine);
  selectedEngineRef.current = selectedEngine;

  const [scanning, setScanning] = useState(false);
  const [scanStatus, setScanStatus] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [ruleHint, setRuleHint] = useState<{
    projectId: number;
    projectName: string;
    path: string;
  } | null>(null);

  const runAction = useCallback(async (fn: () => Promise<void>) => {
    setErrorMsg(null);
    try {
      await fn();
    } catch (err) {
      setErrorMsg(errorText(err));
    }
  }, []);

  const jobs = useReportJobs();
  const daily = usePeriodView(DAILY_LOADERS, selectedEngineRef, setSelectedEngine);
  const weekly = usePeriodView(WEEKLY_LOADERS, selectedEngineRef, setSelectedEngine);
  const monthly = usePeriodView(MONTHLY_LOADERS, selectedEngineRef, setSelectedEngine);
  const project = useProjectView(today);
  const stats = useStatsView();
  const drawer = useSliceDrawer(runAction);

  // Hook setters are stable, so this callback is too.
  const { setSelected: setSelectedDate } = daily;
  const { setSelected: setSelectedWeek } = weekly;
  const { setSelected: setSelectedMonth } = monthly;
  const { setSelectedId: setSelectedProjectId } = project;
  const [batchRevision, setBatchRevision] = useState(0);
  const metadataRequest = useRef(0);
  const refreshMetadata = useCallback(async () => {
    const request = ++metadataRequest.current;
    const [daysRes, weeksRes, monthsRes, p, po, h] = await Promise.all([
      api.listDays(showIgnoredDaysRef.current),
      api.listWeeks(),
      api.listMonths(),
      api.listPrompts(undefined, true),
      api.listProjectsAndRules(),
      api.diagnostics(),
    ]);
    if (request !== metadataRequest.current) return;
    setBatchRevision((revision) => revision + 1);
    setToday(daysRes.today);
    setDays(daysRes.days);
    setSelectedDate((prev) => {
      if (prev) return prev;
      const hasToday = daysRes.days.some((d) => d.localDate === daysRes.today);
      return hasToday ? daysRes.today : (daysRes.days[0]?.localDate ?? daysRes.today);
    });
    setCurrentWeek(weeksRes.currentWeek);
    setWeeks(weeksRes.weeks);
    setSelectedWeek((prev) => prev || (weeksRes.weeks[0]?.week ?? weeksRes.currentWeek));
    setCurrentMonth(monthsRes.currentMonth);
    setMonths(monthsRes.months);
    setSelectedMonth((prev) => prev || (monthsRes.months[0]?.month ?? monthsRes.currentMonth));
    setPrompts(p);
    setProjectsOverview(po);
    setHealth(h);
    setSelectedProjectId((prev) =>
      prev !== null && po.projects.some((proj) => proj.id === prev)
        ? prev
        : (po.projects[0]?.id ?? null),
    );
  }, [setSelectedDate, setSelectedWeek, setSelectedMonth, setSelectedProjectId]);

  /** Reload only the day list (note counts changed). */
  const refreshDays = useCallback(() => {
    api
      .listDays(showIgnoredDaysRef.current)
      .then((r) => setDays(r.days))
      .catch((e) => setErrorMsg(String(e)));
  }, []);

  const refreshAfterReport = async (type: PeriodMode, key: string) => {
    await refreshMetadata();
    const shown = viewModeRef.current;
    if (shown === "daily" && type === shown && daily.selectedRef.current === key) {
      await daily.refresh(key);
    } else if (shown === "weekly" && type === shown && weekly.selectedRef.current === key) {
      await weekly.refresh(key);
    } else if (shown === "monthly") {
      const month = monthly.selectedRef.current;
      if ((type === "monthly" && key === month) || (type === "weekly" && weekMonth(key) === month)) {
        await monthly.refresh(month);
      }
    }
  };

  const refreshCurrent = useCallback(async () => {
    await refreshMetadata();
    if (viewMode === "daily") {
      await daily.refresh(daily.selected);
    } else if (viewMode === "weekly") {
      await weekly.refresh(weekly.selected);
    } else if (viewMode === "monthly") {
      await monthly.refresh(monthly.selected);
    } else if (viewMode === "projects" && project.selectedId !== null) {
      await project.refresh(project.selectedId);
    } else if (viewMode === "stats") {
      await stats.refresh(stats.range, stats.includeIgnored);
    } else if (viewMode === "search") {
      await search.refresh();
    }
  }, [
    refreshMetadata,
    viewMode,
    daily.refresh,
    daily.selected,
    weekly.refresh,
    weekly.selected,
    monthly.refresh,
    monthly.selected,
    project.refresh,
    project.selectedId,
    search.refresh,
    stats.refresh,
    stats.range,
    stats.includeIgnored,
  ]);

  const dayPeriods = useMemo(() => days.map((d) => ({
    key: d.localDate, slices: d.slices, reportStatus: d.reportStatus,
  })), [days]);
  const weekPeriods = useMemo(() => weeks.map((w) => ({
    key: w.week, slices: w.slices, reportStatus: w.reportStatus,
  })), [weeks]);
  const monthPeriods = useMemo(() => months.map((m) => ({
    key: m.month, slices: m.slices, reportStatus: m.reportStatus,
  })), [months]);
  const batchInputs = {
    revision: batchRevision,
    engine: selectedEngine,
    generating: jobs.jobs,
    setErrorMsg,
    startJob: jobs.startJob,
    finishJob: jobs.finishJob,
    refreshAfterReport,
    refreshCurrent,
  };
  const dayBatch = useReportBatch({
    ...batchInputs, reportType: "daily", periods: dayPeriods, prompt: daily.prompt,
  });
  const weekBatch = useReportBatch({
    ...batchInputs, reportType: "weekly", periods: weekPeriods, prompt: weekly.prompt,
  });
  const monthBatch = useReportBatch({
    ...batchInputs, reportType: "monthly", periods: monthPeriods, prompt: monthly.prompt,
  });

  // Prompts, defaults, digests and estimates depend on the report language: reload them
  // when it changes (not on the first render, which loads them anyway).
  const reportLanguageRef = useRef(reportLanguage);
  useEffect(() => {
    if (reportLanguageRef.current === reportLanguage) return;
    reportLanguageRef.current = reportLanguage;
    void runAction(async () => {
      await refreshMetadata();
      await refreshCurrent();
    });
  }, [reportLanguage, runAction, refreshMetadata, refreshCurrent]);

  /** Run a change, then refresh the current view (and the open slice drawer if asked). */
  const mutate = (fn: () => Promise<unknown>, refreshDrawer = false) =>
    runAction(async () => {
      await fn();
      await refreshCurrent();
      if (refreshDrawer) await drawer.refreshIfOpen();
    });

  const preDigest = usePreDigest({
    allowed: viewMode === "daily" && !settingsOpen && !jobs.anyRunning && !scanning,
    workbench: daily.workbench,
    engine: selectedEngine,
    refreshDailyView: daily.refresh,
    runAction,
  });

  useEffect(() => {
    void runAction(refreshMetadata);
  }, [refreshMetadata, runAction]);

  // When the logical day rolls over only "today" and the current week/month move; the selected
  // day stays put, since the previous evening's log is often finished after midnight.
  useDateRollover(() => void runAction(refreshMetadata));

  const periodViews: Record<PeriodMode, PeriodView> = { daily, weekly, monthly };
  useLoadWhenShown(viewMode === "daily", daily, setRuleHint, runAction);
  useLoadWhenShown(viewMode === "weekly", weekly, setRuleHint, runAction);
  useLoadWhenShown(viewMode === "monthly", monthly, setRuleHint, runAction);

  useEffect(() => {
    if (viewMode === "projects" && project.selectedId !== null) {
      void runAction(() => project.refresh(project.selectedId!));
    }
  }, [viewMode, project.selectedId, project.refresh, runAction]);

  useEffect(() => {
    if (viewMode === "stats") {
      setRuleHint(null);
      drawer.close();
      void runAction(() => stats.refresh(stats.range, stats.includeIgnored));
    }
  }, [viewMode, stats.range, stats.includeIgnored, stats.refresh, drawer.close, runAction]);

  // ---- Navigation -------------------------------------------------------

  const openDay = (date: string, showReport = false) => {
    daily.setSelected(date);
    setViewMode("daily");
    if (showReport) daily.setPane("report");
  };
  const openWeek = (week: string, showReport = false) => {
    weekly.setSelected(week);
    setViewMode("weekly");
    if (showReport) weekly.setPane("report");
  };
  const openMonth = (month: string, showReport = false) => {
    monthly.setSelected(month);
    setViewMode("monthly");
    if (showReport) monthly.setPane("report");
  };
  const openProject = (id: number, showReport = false) => {
    project.setSelectedId(id);
    setViewMode("projects");
    if (showReport) project.setPane("rollup");
  };
  const openProjectByName = (name: string) => {
    const proj = projectsOverview?.projects.find((p) => p.name === name);
    if (proj) openProject(proj.id, true);
  };

  // ---- Actions ----------------------------------------------------------

  const handleToggleShowIgnoredDays = () => {
    const next = !showIgnoredDaysRef.current;
    showIgnoredDaysRef.current = next;
    setShowIgnoredDays(next);
    storeItem("aj-show-ignored-days", next ? "1" : "0");
    void runAction(refreshMetadata);
  };

  const generatePeriod = async (mode: PeriodMode, view: PeriodView, period: string, update = false) => {
    jobs.startJob(period, mode === "monthly" ? "stage2" : "stage1");
    const common = { prompt: view.prompt || null, engine: selectedEngine || null, update };
    try {
      const r = await api.generatePeriodReport(mode, period, common);
      console.debug(`[report ${period}] completed`, r);
    } catch (err) {
      const msg = errorText(err);
      // A cancel the user asked for is not an error.
      if (!msg.includes("was cancelled")) setErrorMsg(`${period}: ${msg}`);
    } finally {
      jobs.finishJob(period);
    }
    await runAction(() => refreshAfterReport(mode, period));
  };

  const handleScan = async () => {
    setScanning(true);
    await runAction(async () => {
      const r = await api.scanSessions();
      setScanStatus(tr.scan.result(r.filesParsed, r.newTurns, r.suggestionsPending));
      await refreshCurrent();
    });
    setScanning(false);
  };

  const handleBatchAssign = (sessionIds: number[], projectId: number | null) =>
    mutate(async () => {
      for (const sid of sessionIds) await api.assignSession(sid, projectId);
    }, true);

  const handleBatchUnassign = (sessionIds: number[]) =>
    mutate(async () => {
      for (const sid of sessionIds) await api.unassignSession(sid);
      setRuleHint(null);
    }, true);

  const handleBatchSetIgnoreState = (sliceIds: number[], state: "ignored" | "included") =>
    mutate(async () => {
      for (const id of sliceIds) await api.setSliceIgnoreState(id, state);
    }, true);

  /** Assign a session; offers a path rule when its directory is not covered yet. */
  const assignSession = async (
    sessionId: number,
    projectId: number | null,
    projectName: string | null,
  ) => {
    const res = await api.assignSession(sessionId, projectId);
    setRuleHint(
      res.uncoveredPath && projectId !== null && projectName
        ? { projectId, projectName, path: res.uncoveredPath }
        : null,
    );
  };

  const handleAcceptSuggestions = (sessionIds: number[]) =>
    mutate(() => api.acceptSuggestions({ sessionIds }), true);

  // ---- Rendering --------------------------------------------------------

  const renderSliceList = (wb: DayWorkbench, switcher: React.ReactNode, showDate: boolean) => (
    <SliceList
      date={wb.date}
      showDate={showDate}
      workspaceSwitcher={switcher}
      slices={wb.slices}
      projects={wb.projects}
      selectedSliceId={drawer.sliceId}
      onOpenSlice={(id) => void drawer.open(id)}
      onAssignSession={(sessionId, projectId) =>
        mutate(
          () =>
            assignSession(
              sessionId,
              projectId,
              wb.projects.find((p) => p.id === projectId)?.name ?? null,
            ),
          true,
        )
      }
      onAssignSlice={(sliceId, projectId) =>
        mutate(() => api.assignSlice(sliceId, projectId), true)
      }
      onUnassignSession={(sessionId) => handleBatchUnassign([sessionId])}
      onAcceptSuggestion={handleAcceptSuggestions}
      onBatchAssign={handleBatchAssign}
      onBatchUnassign={handleBatchUnassign}
      onBatchSetIgnoreState={handleBatchSetIgnoreState}
      onQuickCreateProject={(name, description, slice) =>
        mutate(async () => {
          const projectId = await api.createProject({ name, description });
          if (!slice) return;
          if (slice.projectOverrideId !== null) await api.assignSlice(slice.id, projectId);
          else await assignSession(slice.sessionId, projectId, name);
        }, true)
      }
      ruleHint={ruleHint}
      onCreatePathRule={(projectId, path, autoApply) =>
        mutate(async () => {
          await api.addRule({
            projectId,
            ruleType: "path_prefix",
            targetField: null,
            pattern: path,
            autoApply,
          });
          setRuleHint(null);
        })
      }
      onDismissRuleHint={() => setRuleHint(null)}
      onToggleSliceIgnore={(slice: SliceCard) =>
        mutate(
          () => api.setSliceIgnoreState(slice.id, slice.ignored ? "included" : "ignored"),
          true,
        )
      }
    />
  );

  const renderPeriod = (mode: PeriodMode, view: PeriodView) => {
    const wb = view.workbench;
    if (!wb) return null;
    const period = view.selected;
    const switcher = (
      <WorkspaceTabs
        label={tr.workspace.viewLabel(mode)}
        tabs={[
          { key: "report", label: tr.workspace.report },
          { key: "sessions", label: tr.workspace.sessions(activeSliceCount(wb.slices)) },
        ]}
        active={view.pane}
        onChange={view.setPane}
      />
    );
    if (view.pane === "sessions") return renderSliceList(wb, switcher, mode !== "daily");
    const report = { reportType: mode, periodKey: period, projectId: null };
    return (
      <ReportPanel
        kind={mode}
        date={wb.date}
        scopeKey={`${mode}:${period}`}
        projectNames={projectsOverview?.projects.map((p) => p.name)}
        onOpenProject={openProjectByName}
        onOpenWeek={mode === "monthly" ? (w) => openWeek(w, true) : undefined}
        workspaceSwitcher={switcher}
        reportState={view.reportState}
        prompts={wb.prompts}
        engines={wb.engines}
        defaultEngine={wb.defaultEngine}
        selectedPrompt={view.prompt}
        selectedEngine={selectedEngine}
        onSelectPrompt={view.setPrompt}
        onSelectEngine={(engine) => {
          setSelectedEngine(engine);
          void runAction(() => view.refresh(period, engine));
        }}
        generating={period in jobs.jobs}
        genStage={jobs.jobs[period] ?? null}
        onGenerate={(update) => generatePeriod(mode, view, period, update)}
        onCancel={() => api.cancelReportGeneration(period)}
        onSaveEdit={(markdown) => mutate(() => api.saveReportEdit({ ...report, markdown }))}
        onSetStatus={(status) => mutate(() => api.setReportStatus({ ...report, status }))}
        onRestoreVersion={(versionId) =>
          mutate(() => api.restoreReportVersion({ ...report, versionId }))
        }
        onExport={(params) =>
          api.exportReport({ reportType: mode, periodKey: period, projectName: null, ...params })
        }
        onSelectAnchor={(sliceId, turnIndex) => void drawer.open(sliceId, turnIndex)}
        onOpenSessions={() => view.setPane("sessions")}
        projects={projectsOverview?.projects.map((p) => ({ id: p.id, name: p.name, archived: p.archived })) ?? []}
        onOpenPeriod={(key) => (key.includes("W") ? openWeek(key, true) : openDay(key, true))}
        onError={setErrorMsg}
        onNotesChanged={refreshDays}
      />
    );
  };

  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);
  const setProjectArchived = (id: number, archived: boolean) =>
    mutate(() => api.setProjectArchived(id, archived));
  const requestDeleteProject = (id: number) => {
    const p = projectsOverview?.projects.find((x) => x.id === id);
    if (p) setDeleteTarget({ id: p.id, name: p.name, archived: p.archived });
  };
  const deleteProject = (id: number) =>
    runAction(async () => {
      setDeleteTarget(null);
      await api.deleteProject(id);
      if (project.selectedId === id) project.clear();
      await refreshCurrent();
    });

  const renderProject = () => {
    const pwb = project.workbench;
    const rollup = project.rollup;
    const switcher = (
      <WorkspaceTabs
        label={tr.workspace.projectViewLabel}
        tabs={[
          { key: "rollup", label: tr.workspace.report },
          { key: "timeline", label: tr.workspace.sessions(activeSliceCount(pwb?.slices)) },
        ]}
        active={project.pane}
        onChange={project.setPane}
      />
    );
    const current = pwb?.project ?? null;
    const menu = {
      rulesCount: pwb?.rules.length ?? 0,
      archived: current?.archived ?? false,
      onManage: project.openManage,
      onSetArchived: (archived: boolean) => {
        if (current) void setProjectArchived(current.id, archived);
      },
      onDelete: () => {
        if (current) requestDeleteProject(current.id);
      },
    };
    if (project.pane === "rollup" && rollup) {
      return (
        <ProjectRollupPanel
          rollup={rollup}
          workspaceSwitcher={switcher}
          range={project.range}
          onChangeRange={project.setRange}
          onChangeGranularity={project.setGranularity}
          onExport={(params) =>
            api.exportReport({
              reportType: "project",
              periodKey: rollup.periodKey,
              projectName: rollup.projectName,
              ...params,
            })
          }
          onJumpToPeriod={(period) => {
            if (rollup.granularity === "monthly") openMonth(period, true);
            else if (rollup.granularity === "weekly") openWeek(period, true);
            else openDay(period, true);
          }}
          onOpenPeriod={(key) => (key.includes("W") ? openWeek(key, true) : openDay(key, true))}
          onSelectAnchor={(sliceId, turnIndex) => void drawer.open(sliceId, turnIndex)}
          menu={menu}
          onError={setErrorMsg}
          onNotesChanged={refreshDays}
        />
      );
    }
    if (!pwb) {
      return (
        <section className="panel">
          <div className="empty-state">{tr.workspace.selectProject}</div>
        </section>
      );
    }
    return (
      <ProjectTimeline
        workbench={pwb}
        manage={project.manage}
        menu={menu}
        workspaceSwitcher={switcher}
        range={project.range}
        onChangeRange={project.setRange}
        selectedSliceId={drawer.sliceId}
        onOpenSlice={(id) => void drawer.open(id)}
        onRenameAndDescribe={(projectId, newName, description) =>
          mutate(async () => {
            if (newName !== pwb.project.name) await api.renameProject(projectId, newName);
            await api.describeProject(projectId, description);
          })
        }
        onAcceptSuggestion={handleAcceptSuggestions}
        onBatchAssign={handleBatchAssign}
        onBatchUnassign={handleBatchUnassign}
        onBatchSetIgnoreState={handleBatchSetIgnoreState}
        onAddRule={(params) => mutate(() => api.addRule(params))}
        onDeleteRule={(id) => mutate(() => api.deleteRule(id))}
        onSetRuleAuto={(id, autoApply) => mutate(() => api.setRuleAuto(id, autoApply))}
      />
    );
  };

  const exitSearch = useCallback(() => setViewMode(beforeSearchRef.current), []);
  const openReport = (kind: string, periodKey: string) => {
    if (kind === "weekly") openWeek(periodKey, true);
    else if (kind === "monthly") openMonth(periodKey, true);
    else openDay(periodKey, true);
  };

  const renderWorkspace = () => {
    if (viewMode === "search") {
      return (
        <SearchView
          search={search}
          projects={projectsOverview?.projects ?? []}
          agents={preDigest.agentSources}
          onOpenReport={openReport}
          onOpenSlice={(sliceId, turnIndex) => void drawer.open(sliceId, turnIndex)}
          drawerOpen={drawer.detail !== null || drawer.loading}
          onExit={exitSearch}
        />
      );
    }
    if (viewMode === "stats") {
      return (
        <StatsView
          stats={stats.data}
          loading={stats.loading}
          range={stats.range}
          includeIgnored={stats.includeIgnored}
          onChangeRange={stats.setRange}
          onToggleIncludeIgnored={stats.setIncludeIgnored}
          onJumpToDay={(date) => openDay(date)}
          onJumpToWeek={(week) => openWeek(week)}
          onJumpToMonth={(month) => openMonth(month)}
          onJumpToProject={(projectId) => openProject(projectId, true)}
          lantai={stats.lantai}
          lantaiLoading={stats.lantaiLoading}
          lantaiError={stats.lantaiError}
        />
      );
    }
    const content =
      viewMode === "projects"
        ? project.rollup || project.workbench
          ? renderProject()
          : null
        : renderPeriod(viewMode, periodViews[viewMode]);
    if (content) return content;
    const what =
      viewMode === "projects"
        ? "project"
        : viewMode === "monthly"
          ? "month"
          : viewMode === "weekly"
            ? "week"
            : "day";
    return (
      <section className="panel">
        <div className="empty-state">Select a {what}.</div>
      </section>
    );
  };

  return (
    <div className="app-shell">
      <Sidebar
        width={SIDEBAR_WIDTH}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
        days={days}
        today={today}
        selectedDate={daily.selected}
        generating={jobs.jobs}
        onSelectDate={(d) => openDay(d)}
        showIgnoredDays={showIgnoredDays}
        onToggleShowIgnoredDays={handleToggleShowIgnoredDays}
        batches={{ daily: dayBatch, weekly: weekBatch, monthly: monthBatch }}
        weeks={weeks}
        currentWeek={currentWeek}
        selectedWeek={weekly.selected}
        onSelectWeek={(w) => openWeek(w)}
        months={months}
        currentMonth={currentMonth}
        selectedMonth={monthly.selected}
        onSelectMonth={(m) => openMonth(m)}
        projects={projectsOverview?.projects ?? []}
        selectedProjectId={project.selectedId}
        onSelectProject={(id) => openProject(id)}
        onRenameProject={async (id, newName) => {
          // Errors propagate to the inline editor instead of the global banner.
          await api.renameProject(id, newName);
          await refreshMetadata();
          await refreshCurrent();
        }}
        onReorderProjects={(ids) => mutate(() => api.reorderProjects(ids))}
        onSetProjectArchived={(id, archived) => void setProjectArchived(id, archived)}
        onQuickCreateProject={(name, description) =>
          runAction(async () => {
            const newId = await api.createProject({ name, description });
            project.setSelectedId(newId);
            await refreshCurrent();
          })
        }
        scanning={scanning}
        scanStatus={scanStatus}
        onScan={() => void handleScan()}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <div className="pane-divider" aria-hidden />

      <main className="main-area">
        {updates.banner && (
          <UpdateBanner
            banner={updates.banner}
            onDismiss={updates.dismissBanner}
            onSkip={updates.skipVersion}
          />
        )}
        {errorMsg && (
          <ErrorBanner
            message={errorMsg}
            onDismiss={() => setErrorMsg(null)}
            onOpenModels={() => {
              setSettingsTab("models");
              setSettingsOpen(true);
            }}
          />
        )}
        <div className="single-pane">{renderWorkspace()}</div>
      </main>

      <SliceDrawer
        width={drawerSize.value}
        onResizeStart={drawerSize.startDrag(
          (ev, startVal, startX) => startVal + (startX - ev.clientX),
        )}
        detail={drawer.detail}
        loading={drawer.loading}
        projects={
          daily.workbench?.projects ??
          weekly.workbench?.projects ??
          monthly.workbench?.projects ??
          project.workbench?.projects ??
          []
        }
        highlightTurn={drawer.highlightTurn}
        digesting={preDigest.digesting}
        currentModel={
          selectedEngine ||
          (daily.workbench ?? weekly.workbench ?? monthly.workbench)?.defaultEngine ||
          null
        }
        onClose={drawer.close}
        onJumpToTurn={drawer.setHighlightTurn}
        onSaveNote={(sessionId, note) => mutate(() => api.setSessionNote(sessionId, note), true)}
        onSetSliceIgnoreState={(sliceId, state) =>
          mutate(() => api.setSliceIgnoreState(sliceId, state), true)
        }
        onAssignSliceOnly={(sliceId, projectId) =>
          mutate(() => api.assignSlice(sliceId, projectId), true)
        }
        onDigestSlice={async (sliceId) => {
          preDigest.setDigesting(true);
          await mutate(() => api.digestSliceNow(sliceId, selectedEngine || null), true);
          preDigest.setDigesting(false);
        }}
      />

      {deleteTarget && (
        <DeleteProjectDialog
          target={deleteTarget}
          onClose={() => setDeleteTarget(null)}
          onDelete={() => void deleteProject(deleteTarget.id)}
          onArchive={() => {
            setDeleteTarget(null);
            void setProjectArchived(deleteTarget.id, true);
          }}
        />
      )}
      <SettingsModal
        open={settingsOpen}
        onClose={closeSettings}
        tab={settingsTab}
        onChangeTab={setSettingsTab}
        theme={appearance.theme}
        onChangeTheme={appearance.setTheme}
        density={appearance.density}
        onChangeDensity={appearance.setDensity}
        layoutSizes={{ drawer: drawerSize.value }}
        onResetLayout={() => {
          drawerSize.reset();
          resetReportWidth();
          appearance.setDensity("default");
        }}
        prompts={prompts}
        onCreatePrompt={(kind, name, body) => mutate(() => api.createPrompt(kind, name, body))}
        onUpdatePrompt={(id, body) => mutate(() => api.updatePrompt(id, body))}
        onRenamePrompt={(id, newName) => mutate(() => api.renamePrompt(id, newName))}
        onDeletePrompt={(id) => mutate(() => api.deletePrompt(id))}
        onSetDefaultPrompt={(id) => mutate(() => api.setDefaultPrompt(id))}
        projectsOverview={projectsOverview}
        onCreateProject={(params) => mutate(() => api.createProject(params))}
        onRenameProject={(id, newName) => mutate(() => api.renameProject(id, newName))}
        onDescribeProject={(id, description) =>
          mutate(() => api.describeProject(id, description))
        }
        onDeleteProject={requestDeleteProject}
        onSetProjectArchived={(id, archived) => void setProjectArchived(id, archived)}
        onAddRule={(params) => mutate(() => api.addRule(params))}
        onDeleteRule={(id) => mutate(() => api.deleteRule(id))}
        onSetRuleAuto={(id, autoApply) => mutate(() => api.setRuleAuto(id, autoApply))}
        health={health}
        onRefreshHealth={() =>
          runAction(async () => {
            setHealth(await api.diagnostics());
          })
        }
        onModelsChanged={() =>
          runAction(async () => {
            // Re-adopt the (possibly new) default; a renamed or deleted alias must not linger.
            selectedEngineRef.current = "";
            setSelectedEngine("");
            await refreshCurrent();
          })
        }
        agentSources={preDigest.agentSources}
        onUpdateAgent={(agent, enabled, preDigestOn) =>
          runAction(async () => {
            preDigest.updateAgentSources(await api.setAgentSettings(agent, enabled, preDigestOn));
          })
        }
        idleStatus={preDigest.idleStatus}
        onDayBoundaryChanged={refreshCurrent}
        updates={updates}
      />
    </div>
  );
}
