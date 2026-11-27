// What every view shares: the day, week and month lists, prompts, projects and source health,
// loaded together. A refresh also keeps each view's selection valid (picking a first one on
// start, dropping a project that no longer exists).

import { useCallback, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { api } from "../api";
import { errorText } from "../errors";
import { readItem, storeItem } from "../storage";
import type {
  DayOverview,
  Diagnostics,
  MonthOverview,
  ProjectsOverview,
  PromptItem,
  WeekOverview,
} from "../types";

/** The views' selection setters, which a refresh corrects. */
export interface MetadataSelections {
  setSelectedDate: Dispatch<SetStateAction<string>>;
  setSelectedWeek: Dispatch<SetStateAction<string>>;
  setSelectedMonth: Dispatch<SetStateAction<string>>;
  setSelectedProjectId: Dispatch<SetStateAction<number | null>>;
}

const SHOW_IGNORED_DAYS_KEY = "aj-show-ignored-days";

/**
 * `selections` is a ref because the view hooks that own the setters are created after this
 * one (the project view needs `today`); it is read only when a refresh runs.
 */
export function useAppMetadata(
  selections: { current: MetadataSelections | null },
  onError: (message: string) => void,
) {
  const [days, setDays] = useState<DayOverview[]>([]);
  const [today, setToday] = useState("");
  const [showIgnoredDays, setShowIgnoredDaysState] = useState(
    () => readItem(SHOW_IGNORED_DAYS_KEY) === "1",
  );
  // Refreshes read the flag through the ref, so toggling it does not recreate them.
  const showIgnoredDaysRef = useRef(showIgnoredDays);
  showIgnoredDaysRef.current = showIgnoredDays;
  const [weeks, setWeeks] = useState<WeekOverview[]>([]);
  const [currentWeek, setCurrentWeek] = useState("");
  const [months, setMonths] = useState<MonthOverview[]>([]);
  const [currentMonth, setCurrentMonth] = useState("");
  const [prompts, setPrompts] = useState<PromptItem[]>([]);
  const [projectsOverview, setProjectsOverview] = useState<ProjectsOverview | null>(null);
  const [health, setHealth] = useState<Diagnostics | null>(null);
  // Bumped on every refresh, so report batches re-plan from the new lists.
  const [revision, setRevision] = useState(0);

  const request = useRef(0);
  const refresh = useCallback(async () => {
    const current = ++request.current;
    const [daysRes, weeksRes, monthsRes, p, po, h] = await Promise.all([
      api.listDays(showIgnoredDaysRef.current),
      api.listWeeks(),
      api.listMonths(),
      api.listPrompts(undefined, true),
      api.listProjectsAndRules(),
      api.diagnostics(),
    ]);
    if (current !== request.current) return;
    setRevision((r) => r + 1);
    setToday(daysRes.today);
    setDays(daysRes.days);
    setCurrentWeek(weeksRes.currentWeek);
    setWeeks(weeksRes.weeks);
    setCurrentMonth(monthsRes.currentMonth);
    setMonths(monthsRes.months);
    setPrompts(p);
    setProjectsOverview(po);
    setHealth(h);
    const select = selections.current;
    if (!select) return;
    select.setSelectedDate((prev) => {
      if (prev) return prev;
      const hasToday = daysRes.days.some((d) => d.localDate === daysRes.today);
      return hasToday ? daysRes.today : (daysRes.days[0]?.localDate ?? daysRes.today);
    });
    select.setSelectedWeek((prev) => prev || (weeksRes.weeks[0]?.week ?? weeksRes.currentWeek));
    select.setSelectedMonth(
      (prev) => prev || (monthsRes.months[0]?.month ?? monthsRes.currentMonth),
    );
    select.setSelectedProjectId((prev) =>
      prev !== null && po.projects.some((proj) => proj.id === prev)
        ? prev
        : (po.projects[0]?.id ?? null),
    );
  }, [selections]);

  /** Reload only the day list (note counts changed). */
  const refreshDays = useCallback(() => {
    api
      .listDays(showIgnoredDaysRef.current)
      .then((r) => setDays(r.days))
      .catch((e) => onError(errorText(e)));
  }, [onError]);

  /** Reload only the sources' parsing health. */
  const refreshHealth = useCallback(async () => {
    setHealth(await api.diagnostics());
  }, []);

  /** Remember the choice; the caller refreshes to apply it. */
  const setShowIgnoredDays = useCallback((show: boolean) => {
    showIgnoredDaysRef.current = show;
    setShowIgnoredDaysState(show);
    storeItem(SHOW_IGNORED_DAYS_KEY, show ? "1" : "0");
  }, []);

  // The periods each sidebar list plans report batches from.
  const dayPeriods = useMemo(
    () => days.map((d) => ({ key: d.localDate, slices: d.slices, reportStatus: d.reportStatus })),
    [days],
  );
  const weekPeriods = useMemo(
    () => weeks.map((w) => ({ key: w.week, slices: w.slices, reportStatus: w.reportStatus })),
    [weeks],
  );
  const monthPeriods = useMemo(
    () => months.map((m) => ({ key: m.month, slices: m.slices, reportStatus: m.reportStatus })),
    [months],
  );

  return {
    days,
    today,
    showIgnoredDays,
    setShowIgnoredDays,
    weeks,
    currentWeek,
    months,
    currentMonth,
    prompts,
    projectsOverview,
    health,
    revision,
    refresh,
    refreshDays,
    refreshHealth,
    dayPeriods,
    weekPeriods,
    monthPeriods,
  };
}
