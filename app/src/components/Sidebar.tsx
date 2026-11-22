// Left sidebar: view switcher, the calendar and period list of the current browse mode
// (days, weeks, months or projects), and the footer actions. The pieces live in `sidebar/`;
// this component owns the calendar cursor so lists and calendar stay in step.

import { useEffect, useState } from "react";
import { useT } from "../i18n";
import type {
  BatchPlan,
  DayBatchMode,
  DayBatchRun,
  DayOverview,
  GenStage,
  MonthOverview,
  ProjectSummary,
  ViewMode,
  WeekOverview,
} from "../types";
import { DayList } from "./sidebar/DayList";
import { MonthList, WeekList } from "./sidebar/PeriodLists";
import { ProjectsSection } from "./sidebar/ProjectsSection";
import { SidebarCalendar } from "./sidebar/SidebarCalendar";
import { SidebarFooter } from "./sidebar/SidebarFooter";
import { currentYearMonth, weekMonth } from "./sidebar/dates";
import { DayIcon, MonthIcon, ProjectsIcon, WeekIcon } from "./icons";

type BrowseViewMode = Exclude<ViewMode, "stats" | "search">;

const TIME_MODES = ["daily", "weekly", "monthly"] as const;
const TIME_ICONS = { daily: DayIcon, weekly: WeekIcon, monthly: MonthIcon };

function isBrowseMode(m: ViewMode): m is BrowseViewMode {
  return m !== "stats" && m !== "search";
}

interface SidebarProps {
  width: number;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  days: DayOverview[];
  today: string;
  selectedDate: string;
  /** Dates whose daily report is being generated right now, with the stage. */
  generatingDates: Record<string, GenStage>;
  onSelectDate: (date: string) => void;
  showIgnoredDays: boolean;
  onToggleShowIgnoredDays: () => void;
  daySelectMode: boolean;
  selectedDates: string[];
  onChangeDaySelectMode: (on: boolean) => void;
  onChangeSelectedDates: (dates: string[]) => void;
  onQuickSelectStale: () => Promise<void>;
  dayBatchPlan: BatchPlan | null;
  dayBatchPlanning: boolean;
  dayBatchIncludeFinal: boolean;
  onChangeDayBatchIncludeFinal: (include: boolean) => void;
  dayBatchRun: DayBatchRun | null;
  dayBatchSummary: string | null;
  onRunDayBatch: (mode: DayBatchMode) => Promise<void>;
  onCancelDayBatch: () => void;
  onSetDayBatchStatus: (status: "draft" | "final") => Promise<void>;
  onCloseDayBatch: () => void;
  weeks: WeekOverview[];
  currentWeek: string;
  selectedWeek: string;
  onSelectWeek: (week: string) => void;
  months: MonthOverview[];
  currentMonth: string;
  selectedMonth: string;
  onSelectMonth: (month: string) => void;
  projects: ProjectSummary[];
  selectedProjectId: number | null;
  onSelectProject: (id: number) => void;
  onRenameProject: (id: number, newName: string) => Promise<void>;
  onReorderProjects: (ids: number[]) => Promise<void>;
  onSetProjectArchived: (id: number, archived: boolean) => void;
  onQuickCreateProject: (
    name: string,
    description: string | null,
  ) => Promise<void>;
  scanning: boolean;
  scanStatus: string | null;
  onScan: () => void;
  onOpenSettings: () => void;
}

export function Sidebar(props: SidebarProps) {
  const {
    viewMode,
    onViewModeChange,
    today,
    selectedDate,
    selectedWeek,
    currentWeek,
    selectedMonth,
    currentMonth,
  } = props;
  const tr = useT();
  const [month, setMonth] = useState(
    () => (selectedDate || today).slice(0, 7) || currentYearMonth(),
  );
  const [year, setYear] = useState(
    () => (selectedMonth || currentMonth || today).slice(0, 4) || String(new Date().getFullYear()),
  );

  // Stats and Search are shown beside the sidebar; the sidebar keeps showing the last browse mode.
  const [lastBrowseMode, setLastBrowseMode] = useState<BrowseViewMode>("daily");
  useEffect(() => {
    if (isBrowseMode(viewMode)) setLastBrowseMode(viewMode);
  }, [viewMode]);
  const browseMode: BrowseViewMode = isBrowseMode(viewMode) ? viewMode : lastBrowseMode;


  // Move the calendar to the selection whenever it or the mode changes.
  useEffect(() => {
    if (browseMode === "monthly") {
      const ym = selectedMonth || currentMonth;
      if (/^\d{4}-\d{2}$/.test(ym)) setYear(ym.slice(0, 4));
    } else if (browseMode === "weekly") {
      const wm = weekMonth(selectedWeek || currentWeek);
      if (wm) setMonth(wm);
    } else if (selectedDate) {
      setMonth(selectedDate.slice(0, 7));
    } else if (today) {
      setMonth(today.slice(0, 7));
    }
  }, [browseMode, selectedDate, today, selectedWeek, currentWeek, selectedMonth, currentMonth]);

  const activeMonth = /^\d{4}-\d{2}$/.test(month)
    ? month
    : (selectedDate || today).slice(0, 7) || currentYearMonth();
  const activeYear = /^\d{4}$/.test(year)
    ? year
    : (selectedMonth || currentMonth || today).slice(0, 4) || String(new Date().getFullYear());

  return (
    <aside className="sidebar" style={{ width: `${props.width}px` }}>
      {/* Time views (day, week, month), then the project view. */}
      <div className="view-switch" role="tablist">
        <div className="pill-group">
          {TIME_MODES.map((mode) => {
            const Icon = TIME_ICONS[mode];
            return (
              <button
                key={mode}
                type="button"
                role="tab"
                aria-selected={viewMode === mode}
                aria-label={tr.views[mode]}
                data-tip={tr.views[mode]}
                data-tip-align={mode === "daily" ? "start" : undefined}
                className={`pill ${viewMode === mode ? "active" : ""}`}
                onClick={() => onViewModeChange(mode)}
              >
                <Icon />
              </button>
            );
          })}
        </div>
        <span className="view-switch-sep" aria-hidden="true" />
        <div className="pill-group view-switch-projects">
          <button
            type="button"
            role="tab"
            aria-selected={viewMode === "projects"}
            aria-label={tr.views.projects}
            data-tip={tr.views.projects}
            data-tip-align="end"
            className={`pill ${viewMode === "projects" ? "active" : ""}`}
            onClick={() => onViewModeChange("projects")}
          >
            <ProjectsIcon />
          </button>
        </div>
      </div>

      {browseMode !== "projects" && (
        <SidebarCalendar
          mode={browseMode}
          month={activeMonth}
          setMonth={setMonth}
          year={activeYear}
          setYear={setYear}
          today={today}
          days={props.days}
          selectedDate={selectedDate}
          onSelectDate={props.onSelectDate}
          weeks={props.weeks}
          currentWeek={currentWeek}
          selectedWeek={selectedWeek}
          onSelectWeek={props.onSelectWeek}
          months={props.months}
          currentMonth={currentMonth}
          selectedMonth={selectedMonth}
          onSelectMonth={props.onSelectMonth}
        />
      )}

      {browseMode === "daily" ? (
        <DayList
          days={props.days}
          today={today}
          currentWeek={currentWeek}
          selectedDate={selectedDate}
          generating={props.generatingDates}
          onOpenDate={(date) => {
            setMonth(date.slice(0, 7));
            props.onSelectDate(date);
          }}
          showIgnoredDays={props.showIgnoredDays}
          onToggleShowIgnoredDays={props.onToggleShowIgnoredDays}
          escapeCloses={viewMode === "daily"}
          selectMode={props.daySelectMode}
          selectedDates={props.selectedDates}
          onChangeSelectMode={props.onChangeDaySelectMode}
          onChangeSelectedDates={props.onChangeSelectedDates}
          onQuickSelectStale={props.onQuickSelectStale}
          batchPlan={props.dayBatchPlan}
          batchPlanning={props.dayBatchPlanning}
          batchIncludeFinal={props.dayBatchIncludeFinal}
          onChangeBatchIncludeFinal={props.onChangeDayBatchIncludeFinal}
          batchRun={props.dayBatchRun}
          batchSummary={props.dayBatchSummary}
          onRunBatch={props.onRunDayBatch}
          onCancelBatch={props.onCancelDayBatch}
          onSetBatchStatus={props.onSetDayBatchStatus}
          onCloseBatch={props.onCloseDayBatch}
        />
      ) : browseMode === "weekly" ? (
        <WeekList
          weeks={props.weeks}
          currentWeek={currentWeek}
          selectedWeek={selectedWeek}
          generating={props.generatingDates}
          onOpenWeek={(week) => {
            setMonth(weekMonth(week) ?? activeMonth);
            props.onSelectWeek(week);
          }}
        />
      ) : browseMode === "monthly" ? (
        <MonthList
          months={props.months}
          currentMonth={currentMonth}
          selectedMonth={selectedMonth}
          generating={props.generatingDates}
          onOpenMonth={(m) => {
            setYear(m.slice(0, 4));
            props.onSelectMonth(m);
          }}
        />
      ) : (
        <ProjectsSection
          projects={props.projects}
          selectedProjectId={props.selectedProjectId}
          onSelectProject={props.onSelectProject}
          onRenameProject={props.onRenameProject}
          onReorderProjects={props.onReorderProjects}
          onSetArchived={props.onSetProjectArchived}
          onQuickCreateProject={props.onQuickCreateProject}
        />
      )}

      <SidebarFooter
        scanning={props.scanning}
        scanStatus={props.scanStatus}
        onScan={props.onScan}
        statsActive={viewMode === "stats"}
        onToggleStats={() => onViewModeChange(viewMode === "stats" ? lastBrowseMode : "stats")}
        searchActive={viewMode === "search"}
        onToggleSearch={() => onViewModeChange(viewMode === "search" ? lastBrowseMode : "search")}
        onOpenSettings={props.onOpenSettings}
      />
    </aside>
  );
}
