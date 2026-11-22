// Report generation jobs in flight, keyed by period (date, week or month) with their stage.
// Tracks backend events so the state survives switching views and webview reloads.

import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { GenStage, ReportProgress } from "../types";

function stageOf(p: ReportProgress): GenStage {
  return p.stage === "digestsPending" || p.stage === "sliceDigested" ? "stage1" : "stage2";
}

export function useReportJobs() {
  const [jobs, setJobs] = useState<Record<string, GenStage>>({});

  /** Mark a job as running; `keepStage` leaves a stage reported earlier untouched. */
  const startJob = useCallback((key: string, stage: GenStage, keepStage = false) => {
    setJobs((prev) => ({ ...prev, [key]: keepStage ? (prev[key] ?? stage) : stage }));
  }, []);

  const finishJob = useCallback((key: string) => {
    setJobs((prev) => {
      if (!(key in prev)) return prev;
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }, []);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const keep = (p: Promise<() => void>) =>
      void p.then((fn) => (disposed ? fn() : unlisteners.push(fn)));
    keep(
      api.onReportProgress(({ key, progress }) => {
        console.debug(`[report ${key}]`, progress);
        setJobs((prev) => ({ ...prev, [key]: stageOf(progress) }));
      }),
    );
    keep(api.onReportJobFinished(finishJob));
    void api.runningReportJobs().then((keys) =>
      setJobs((prev) => {
        const next = { ...prev };
        for (const k of keys) next[k] ??= "stage1";
        return next;
      }),
    );
    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [finishJob]);

  return { jobs, anyRunning: Object.keys(jobs).length > 0, startJob, finishJob };
}
