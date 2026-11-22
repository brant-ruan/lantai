// Multi-day selection in the sidebar and the batch operations on it: planning which days
// need generating or updating, running them through a bounded worker pool, and bulk status
// changes.

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n";
import type { BatchPlan, DayBatchMode, DayBatchRun, DayOverview, GenStage } from "../types";

export interface DayBatchInputs {
  days: DayOverview[];
  /** When ignored days are hidden, selected dates that disappear are dropped. */
  showIgnoredDays: boolean;
  prompt: string;
  engine: string;
  runAction: (fn: () => Promise<void>) => Promise<void>;
  setErrorMsg: (msg: string | null) => void;
  startJob: (key: string, stage: GenStage, keepStage?: boolean) => void;
  finishJob: (key: string) => void;
  /** Refresh after one day finished: the day list, and the report if that day is on screen. */
  refreshAfterDay: (date: string) => Promise<void>;
  refreshCurrent: () => Promise<void>;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Which days a batch mode runs, and whether each is an incremental update. */
function planJobs(plan: BatchPlan, mode: DayBatchMode) {
  const jobs: Array<{ date: string; update: boolean }> = [];
  let skipped = 0;
  for (const day of plan.days) {
    const generate = day.action === "generate";
    const update = day.action === "update";
    const run =
      mode === "smart"
        ? generate || update
        : mode === "stale"
          ? update
          : mode === "missing"
            ? generate
            : day.canRegenerate;
    if (!run) {
      skipped += 1;
    } else {
      jobs.push({ date: day.date, update: mode !== "regenerate" && update });
    }
  }
  return { jobs, skipped };
}

export function useDayBatch(inputs: DayBatchInputs) {
  const { days, showIgnoredDays, runAction, setErrorMsg } = inputs;
  const ts = useT().dayBatchSummary;
  const [selectMode, setSelectMode] = useState(false);
  const [selectedDates, setSelectedDates] = useState<string[]>([]);
  const [includeFinal, setIncludeFinal] = useState(false);
  const [plan, setPlan] = useState<BatchPlan | null>(null);
  const [planning, setPlanning] = useState(false);
  const [run, setRun] = useState<DayBatchRun | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const [planRevision, setPlanRevision] = useState(0);
  const planReqRef = useRef(0);
  const cancelRef = useRef(false);
  const activeDatesRef = useRef(new Set<string>());
  // The worker pool outlives renders; it reads the latest inputs through a ref.
  const inputsRef = useRef(inputs);
  inputsRef.current = inputs;

  const bumpPlanRevision = useCallback(() => setPlanRevision((n) => n + 1), []);

  useEffect(() => {
    if (showIgnoredDays) return;
    const visible = new Set(days.map((d) => d.localDate));
    setSelectedDates((prev) =>
      prev.every((d) => visible.has(d)) ? prev : prev.filter((d) => visible.has(d)),
    );
  }, [days, showIgnoredDays]);

  useEffect(() => {
    if (selectedDates.length === 0) {
      planReqRef.current += 1;
      setPlan(null);
      setPlanning(false);
      return;
    }
    const req = ++planReqRef.current;
    setPlanning(true);
    void api
      .planBatch(selectedDates, includeFinal)
      .then((p) => {
        if (req === planReqRef.current) setPlan(p);
      })
      .catch((err) => {
        if (req === planReqRef.current) setErrorMsg(errorText(err));
      })
      .finally(() => {
        if (req === planReqRef.current) setPlanning(false);
      });
  }, [selectedDates, includeFinal, planRevision, setErrorMsg]);

  const changeSelectMode = (on: boolean) => {
    if (run !== null) return;
    setSelectMode(on);
    if (!on) {
      setSelectedDates([]);
      setPlan(null);
      setSummary(null);
    }
  };

  const changeSelectedDates = (dates: string[]) => {
    setSelectedDates(dates);
    setSummary(null);
  };

  const close = () => changeSelectMode(false);

  const quickSelectStale = async () => {
    if (run !== null) return;
    await runAction(async () => {
      const candidates = days
        .filter((d) => d.slices > 0 && d.reportStatus !== null)
        .map((d) => d.localDate);
      setSelectMode(true);
      setSummary(null);
      if (candidates.length === 0) {
        setSelectedDates([]);
        setSummary(ts.noReports);
        return;
      }
      setPlanning(true);
      try {
        const p = await api.planBatch(candidates, true);
        const staleDates = p.days.filter((d) => d.stale && d.slicesUsed > 0).map((d) => d.date);
        setSelectedDates(staleDates);
        if (staleDates.length === 0) setSummary(ts.allUpToDate);
      } finally {
        setPlanning(false);
      }
    });
  };

  const cancel = () => {
    cancelRef.current = true;
    for (const date of activeDatesRef.current) {
      void api.cancelReportGeneration(date);
    }
  };

  const runBatch = async (mode: DayBatchMode) => {
    if (run !== null || selectedDates.length === 0) return;
    setErrorMsg(null);
    setSummary(null);
    let fresh: BatchPlan;
    try {
      fresh = await api.planBatch(selectedDates, includeFinal);
    } catch (err) {
      setErrorMsg(errorText(err));
      return;
    }
    const { jobs, skipped } = planJobs(fresh, mode);
    if (jobs.length === 0) {
      setSummary(skipped > 0 ? ts.skippedNothing(skipped) : ts.nothingToRun);
      return;
    }

    cancelRef.current = false;
    activeDatesRef.current = new Set();
    setRun({ done: 0, total: jobs.length });

    let cursor = 0;
    let started = 0;
    let generated = 0;
    let updated = 0;
    let cancelled = 0;
    const failed: string[] = [];
    const workerCount = Math.min(Math.max(1, fresh.batchConcurrency), jobs.length);

    const worker = async () => {
      while (!cancelRef.current) {
        const idx = cursor++;
        if (idx >= jobs.length) return;
        started += 1;
        const job = jobs[idx];
        const cur = inputsRef.current;
        let alreadyRunning = false;
        activeDatesRef.current.add(job.date);
        cur.startJob(job.date, "stage1", true);
        try {
          await api.generateDailyReport({
            date: job.date,
            project: null,
            prompt: cur.prompt || null,
            engine: cur.engine || null,
            update: job.update,
          });
          if (job.update) updated += 1;
          else generated += 1;
        } catch (err) {
          const msg = errorText(err);
          if (msg.includes("already being generated")) {
            alreadyRunning = true;
            failed.push(job.date);
          } else if (msg.includes("was cancelled")) {
            cancelled += 1;
          } else {
            failed.push(job.date);
          }
        } finally {
          activeDatesRef.current.delete(job.date);
          if (!alreadyRunning) inputsRef.current.finishJob(job.date);
          setRun((prev) => (prev ? { ...prev, done: prev.done + 1 } : null));
          try {
            await inputsRef.current.refreshAfterDay(job.date);
          } catch {
            // Keep the batch moving; overall status is reported at the end.
          }
        }
      }
    };

    await Promise.all(Array.from({ length: workerCount }, () => worker()));
    cancelled += Math.max(0, jobs.length - started);
    setRun(null);

    const parts: string[] = [];
    if (generated > 0) parts.push(ts.generated(generated));
    if (updated > 0) parts.push(ts.updated(updated));
    if (skipped > 0) parts.push(ts.skipped(skipped));
    if (cancelled > 0) parts.push(ts.cancelled(cancelled));
    if (failed.length > 0) parts.push(ts.failed(failed.length));
    setSummary(parts.join(" · ") || ts.done);
    setSelectedDates(failed);
    bumpPlanRevision();
  };

  const setStatus = async (status: "draft" | "final") => {
    if (selectedDates.length === 0 || run !== null) return;
    await runAction(async () => {
      const res = await api.setReportsStatus(selectedDates, status);
      const parts = [ts.marked(res.updated.length, status)];
      if (res.skipped.length > 0) parts.push(ts.skippedNoReport(res.skipped.length));
      setSummary(parts.join(" · "));
      await inputsRef.current.refreshCurrent();
    });
  };

  return {
    selectMode,
    selectedDates,
    includeFinal,
    setIncludeFinal,
    plan,
    planning,
    run,
    summary,
    bumpPlanRevision,
    changeSelectMode,
    changeSelectedDates,
    close,
    quickSelectStale,
    cancel,
    runBatch,
    setStatus,
  };
}
