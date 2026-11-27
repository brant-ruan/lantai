// State of the project view: the selected project, its time range and rollup granularity,
// and the loaded workbench and rollup.

import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type {
  ProjectRangeKey,
  ProjectRollup,
  ProjectWorkbench,
  RollupGranularity,
} from "../types";
import { readChoice, storeItem } from "../storage";

export type ProjectPane = "rollup" | "timeline";
/** A management card open above the project's sessions. */
export type ProjectManage = "details" | "rules" | null;

function shiftDate(ymd: string, deltaDays: number): string {
  const [y, m, d] = ymd.split("-").map(Number);
  const dt = new Date(y, m - 1, d + deltaDays);
  const yyyy = dt.getFullYear();
  const mm = String(dt.getMonth() + 1).padStart(2, "0");
  const dd = String(dt.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

function projectRangeParams(today: string, range: ProjectRangeKey, granularity: RollupGranularity) {
  // The period key also names the export file, so it says how entries are grouped.
  const suffix = granularity === "weekly" ? "-weekly" : granularity === "monthly" ? "-monthly" : "";
  if (range === "all" || !today) {
    return { sinceDate: null, untilDate: null, periodKey: `all${suffix}` };
  }
  const days = range === "7d" ? 6 : range === "14d" ? 13 : 29;
  return {
    sinceDate: shiftDate(today, -days),
    untilDate: null,
    periodKey: `last-${range}${suffix}`,
  };
}

export function useProjectView(today: string) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [range, setRange] = useState<ProjectRangeKey>("30d");
  const [granularity, setGranularity] = useState<RollupGranularity>(() =>
    readChoice("aj-rollup-granularity", ["daily", "weekly", "monthly"], "daily"),
  );
  useEffect(() => storeItem("aj-rollup-granularity", granularity), [granularity]);
  const [workbench, setWorkbench] = useState<ProjectWorkbench | null>(null);
  const [rollup, setRollup] = useState<ProjectRollup | null>(null);
  const [pane, setPane] = useState<ProjectPane>("rollup");
  const [manage, setManage] = useState<ProjectManage>(null);
  /** Open a management card; it lives on the sessions tab. */
  const openManage = useCallback((m: ProjectManage) => {
    setManage(m);
    if (m) setPane("timeline");
  }, []);

  const params = useMemo(
    () => projectRangeParams(today, range, granularity),
    [today, range, granularity],
  );

  const refresh = useCallback(
    async (projectId: number) => {
      const [pwb, r] = await Promise.all([
        api.projectWorkbench(projectId, params.sinceDate, params.untilDate),
        api.getProjectRollup({
          projectId,
          periodKey: params.periodKey,
          granularity,
          sinceDate: params.sinceDate,
          untilDate: params.untilDate,
        }),
      ]);
      setWorkbench(pwb);
      setRollup(r);
    },
    [params, granularity],
  );

  const clear = useCallback(() => {
    setSelectedId(null);
    setManage(null);
    setWorkbench(null);
    setRollup(null);
  }, []);

  return {
    selectedId,
    setSelectedId,
    range,
    setRange,
    setGranularity,
    workbench,
    rollup,
    pane,
    setPane,
    manage,
    openManage,
    refresh,
    clear,
  };
}
