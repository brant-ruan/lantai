// Plans and runs one sidebar report batch, keeping selection and progress for each report kind.

import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n";
import type { BatchItem, BatchPlan, GenStage, PeriodMode, ReportBatchMode, ReportBatchRun } from "../types";
import { errorText } from "../errors";

interface BatchInputs {
  reportType: PeriodMode;
  revision: number;
  periods: { key: string; slices: number; reportStatus: "draft" | "final" | null }[];
  prompt: string;
  engine: string;
  generating: Record<string, GenStage>;
  setErrorMsg: (msg: string | null) => void;
  startJob: (key: string, stage: GenStage, keepStage?: boolean) => void;
  finishJob: (key: string) => void;
  refreshAfterReport: (type: PeriodMode, key: string) => Promise<void>;
  refreshCurrent: () => Promise<void>;
}


function shouldRun(item: BatchItem, mode: ReportBatchMode) {
  switch (mode) {
    case "smart": return item.action !== "skip";
    case "stale": return item.action === "update";
    case "missing": return item.action === "generate";
    case "regenerate": return item.canRegenerate;
  }
}

function monthlyCoverage(plan: BatchPlan | null) {
  return plan?.items
    .filter((item) => !item.hasInputs || item.missingWeeks.length > 0)
    .map(({ periodKey, hasInputs, missingWeeks }) => [periodKey, hasInputs, missingWeeks]);
}

export function useReportBatch(inputs: BatchInputs) {
  const { reportType, periods, revision, setErrorMsg } = inputs;
  const ts = useT().reportBatchSummary;
  const [selectMode, setSelectMode] = useState(false);
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  const [includeFinal, setIncludeFinal] = useState(false);
  const [preview, setPreview] = useState<{ request: string; plan: BatchPlan | null } | null>(null);
  const [run, setRun] = useState<ReportBatchRun | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Lock before awaiting the fresh plan, so repeat clicks cannot start another operation.
  const busyRef = useRef(false);
  const cancelled = useRef(false);
  const activeKeys = useRef(new Set<string>());
  const inputsRef = useRef(inputs);
  inputsRef.current = inputs;

  const request = JSON.stringify([selectedKeys, includeFinal, revision]);
  const plan = preview?.request === request ? preview.plan : null;
  const planning = selectedKeys.length > 0 && preview?.request !== request;

  useEffect(() => {
    if (busy) return;
    const visible = new Set(periods.map(({ key }) => key));
    setSelectedKeys((prev) =>
      prev.every((key) => visible.has(key)) ? prev : prev.filter((key) => visible.has(key)),
    );
  }, [periods, busy]);

  useEffect(() => {
    if (!planning || busy) return;
    let obsolete = false;
    void api.planBatch(reportType, selectedKeys, includeFinal).then(
      (plan) => {
        if (!obsolete) setPreview({ request, plan });
      },
      (err: unknown) => {
        if (obsolete) return;
        setErrorMsg(errorText(err));
        setPreview({ request, plan: null });
      },
    );
    return () => { obsolete = true; };
  }, [reportType, selectedKeys, includeFinal, request, planning, busy, setErrorMsg]);

  const changeSelectMode = (on: boolean) => {
    if (busyRef.current) return;
    setSelectMode(on);
    if (!on) {
      setSelectedKeys([]);
      setSummary(null);
    }
  };

  const changeSelection = (keys: string[]) => {
    if (busyRef.current) return;
    setSelectMode(true);
    setSelectedKeys(keys);
    setSummary(null);
  };

  const perform = async (action: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setErrorMsg(null);
    try {
      await action();
    } catch (err) {
      setErrorMsg(errorText(err));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  const quickSelectStale = () => perform(async () => {
    const candidates = periods
      .filter((p) => p.slices > 0 && p.reportStatus !== null)
      .map((p) => p.key);
    setSelectMode(true);
    setSummary(null);
    if (candidates.length === 0) {
      setSelectedKeys([]);
      setSummary(ts.noReports);
      return;
    }
    const all = await api.planBatch(reportType, candidates, true);
    const stale = all.items.filter((item) => item.stale && item.hasInputs).map((item) => item.periodKey);
    setSelectedKeys(stale);
    if (stale.length === 0) setSummary(ts.allUpToDate);
  });

  const cancel = () => {
    cancelled.current = true;
    for (const key of activeKeys.current) void api.cancelReportGeneration(key);
  };

  const runBatch = (mode: ReportBatchMode) => perform(async () => {
    if (selectedKeys.length === 0) return;
    setSummary(null);
    const fresh = await api.planBatch(reportType, selectedKeys, includeFinal);
    setPreview({ request, plan: fresh });
    if (reportType === "monthly" &&
        JSON.stringify(monthlyCoverage(fresh)) !== JSON.stringify(monthlyCoverage(plan))) {
      setSummary(ts.coverageChanged);
      return;
    }

    const queue = fresh.items.filter((item) => shouldRun(item, mode));
    const skipped = fresh.items.length - queue.length;
    if (queue.length === 0) {
      setSummary(skipped > 0 ? ts.skippedNothing(skipped) : ts.nothingToRun);
      return;
    }

    const { prompt, engine } = inputs;
    const failed = new Map<string, string>();
    cancelled.current = false;
    setRun({ done: 0, total: queue.length });

    let cursor = 0;
    let generated = 0;
    let updated = 0;
    let cancelledCount = 0;
    const workerCount = Math.min(Math.max(1, fresh.batchConcurrency), queue.length);

    const worker = async () => {
      while (!cancelled.current && cursor < queue.length) {
        const item = queue[cursor++];
        const key = item.periodKey;
        const cur = inputsRef.current;
        const update = mode !== "regenerate" && item.action === "update";
        let ownsJob = false;
        try {
          if (cur.generating[key]) {
            failed.set(key, ts.alreadyRunning);
            continue;
          }
          ownsJob = true;
          activeKeys.current.add(key);
          cur.startJob(key, reportType === "monthly" ? "stage2" : "stage1", true);
          const result = await api.generatePeriodReport(reportType, key, {
            prompt: prompt || null,
            engine: engine || null,
            update,
          });
          if (result.updated) updated += 1;
          else generated += 1;
        } catch (err) {
          const msg = errorText(err);
          if (msg.includes("already being generated")) ownsJob = false;
          if (msg.includes("was cancelled")) cancelledCount += 1;
          else failed.set(key, msg);
        } finally {
          activeKeys.current.delete(key);
          if (ownsJob) cur.finishJob(key);
          setRun((prev) => prev && { ...prev, done: prev.done + 1 });
          if (ownsJob) {
            try {
              await inputsRef.current.refreshAfterReport(reportType, key);
            } catch (err) {
              setErrorMsg(errorText(err));
            }
          }
        }
      }
    };

    try {
      await Promise.all(Array.from({ length: workerCount }, () => worker()));
    } finally {
      setRun(null);
    }
    cancelledCount += queue.length - cursor;
    const parts: string[] = [];
    if (generated > 0) parts.push(ts.generated(generated));
    if (updated > 0) parts.push(ts.updated(updated));
    if (skipped > 0) parts.push(ts.skipped(skipped));
    if (cancelledCount > 0) parts.push(ts.cancelled(cancelledCount));
    if (failed.size > 0) parts.push(ts.failed(failed.size));
    setSummary(parts.join(" · ") || ts.done);
    const retry = queue.map((item) => item.periodKey).filter((key) => failed.has(key));
    const blocked = reportType === "monthly"
      ? fresh.items.filter((item) => !item.hasInputs).map((item) => item.periodKey)
      : [];
    setSelectedKeys([...retry, ...blocked]);
    const [firstFailure] = retry;
    if (firstFailure) setErrorMsg(`${firstFailure}: ${failed.get(firstFailure)}`);
  });

  const setStatus = (status: "draft" | "final") => perform(async () => {
    if (selectedKeys.length === 0) return;
    const res = await api.setReportsStatus(reportType, selectedKeys, status);
    const parts = [ts.marked(res.updated.length, status)];
    if (res.skipped.length > 0) parts.push(ts.skippedNoReport(res.skipped.length));
    setSummary(parts.join(" · "));
    await inputsRef.current.refreshCurrent();
  });

  return {
    selectMode,
    selectedKeys,
    includeFinal,
    setIncludeFinal,
    plan,
    planning,
    busy,
    run,
    summary,
    changeSelectMode,
    changeSelection,
    close: () => changeSelectMode(false),
    quickSelectStale,
    cancel,
    runBatch,
    setStatus,
  };
}

export type ReportBatch = ReturnType<typeof useReportBatch>;
