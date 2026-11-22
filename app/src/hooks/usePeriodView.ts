// State of one report period view (daily, weekly or monthly): the selected period, its
// workbench, report state, chosen prompt and active pane. The three views differ only in
// how the workbench and report state are loaded.

import { useCallback, useRef, useState, type MutableRefObject } from "react";
import type { DailyReportState, DayWorkbench } from "../types";

export interface PeriodLoaders {
  workbench: (period: string) => Promise<DayWorkbench>;
  reportState: (period: string, engine: string | null) => Promise<DailyReportState>;
}

export type PeriodPane = "report" | "sessions";

export function usePeriodView(
  loaders: PeriodLoaders,
  engineRef: MutableRefObject<string>,
  setSelectedEngine: (engine: string) => void,
) {
  const [selected, setSelected] = useState("");
  const [workbench, setWorkbench] = useState<DayWorkbench | null>(null);
  const [reportState, setReportState] = useState<DailyReportState | null>(null);
  const [prompt, setPrompt] = useState("");
  const [pane, setPane] = useState<PeriodPane>("report");
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  // Monotonic request id: a slower response for a previously selected period must not
  // overwrite the one on screen.
  const reqRef = useRef(0);
  const loadersRef = useRef(loaders);
  loadersRef.current = loaders;

  // Stable: reads the engine through a ref so changing it does not refetch twice.
  const refresh = useCallback(
    async (period: string, engineOverride?: string) => {
      if (!period) return;
      const req = ++reqRef.current;
      const wb = await loadersRef.current.workbench(period);
      if (req !== reqRef.current) return;
      const currentEngine = engineRef.current;
      const engineToUse = engineOverride ?? (currentEngine || wb.defaultEngine);
      if (!currentEngine && wb.defaultEngine) setSelectedEngine(wb.defaultEngine);
      const defPrompt = wb.prompts.find((p) => p.isDefault)?.name ?? wb.prompts[0]?.name ?? "";
      setPrompt((prev) => (prev && wb.prompts.some((p) => p.name === prev) ? prev : defPrompt));
      const rep = await loadersRef.current.reportState(period, engineToUse || null);
      if (req !== reqRef.current) return;
      setWorkbench(wb);
      setReportState(rep);
    },
    [engineRef, setSelectedEngine],
  );

  return {
    selected,
    setSelected,
    selectedRef,
    workbench,
    reportState,
    prompt,
    setPrompt,
    pane,
    setPane,
    refresh,
  };
}

export type PeriodView = ReturnType<typeof usePeriodView>;
