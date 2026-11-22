import { useEffect, useState } from "react";
import { useT } from "../i18n";
import type { BatchPlan, DayBatchMode, DayBatchRun } from "../types";
import { DropdownMenu } from "./DropdownMenu";

interface DayBatchBarProps {
  selectedCount: number;
  plan: BatchPlan | null;
  planning: boolean;
  includeFinal: boolean;
  onChangeIncludeFinal: (include: boolean) => void;
  batchRun: DayBatchRun | null;
  batchSummary: string | null;
  onRunBatch: (mode: DayBatchMode) => Promise<void>;
  onCancelBatch: () => void;
  onSetStatus: (status: "draft" | "final") => Promise<void>;
  onClose: () => void;
}

export function DayBatchBar({
  selectedCount,
  plan,
  planning,
  includeFinal,
  onChangeIncludeFinal,
  batchRun,
  batchSummary,
  onRunBatch,
  onCancelBatch,
  onSetStatus,
  onClose,
}: DayBatchBarProps) {
  const tr = useT();
  const t = tr.dayBatch;
  const [confirmRegenerate, setConfirmRegenerate] = useState(false);

  useEffect(() => {
    setConfirmRegenerate(false);
  }, [selectedCount, batchRun, includeFinal]);

  const missingCount = plan?.missingCount ?? 0;
  const staleCount = plan?.staleCount ?? 0;
  const regenerateCount = plan?.regenerateCount ?? 0;
  const smartCount = missingCount + staleCount;

  if (batchRun !== null) {
    return (
      <div className="day-batch-bar">
        <div className="day-batch-row">
          <span className="batch-count">
            {t.running(batchRun.done, batchRun.total)}
          </span>
          <button
            type="button"
            className="btn btn-danger btn-xs"
            onClick={onCancelBatch}
          >
            {t.cancelAll}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="day-batch-bar">
      <div className="day-batch-row">
        <span className="batch-count">
          {t.selected(selectedCount)}
          {planning ? "…" : ""}
        </span>
        <button
          type="button"
          className="btn btn-ghost btn-xs day-batch-close"
          onClick={onClose}
          title={t.exitTitle}
          aria-label={t.exit}
        >
          ×
        </button>
      </div>

      {confirmRegenerate && (
        <div className="day-batch-confirm">
          <span>{t.confirmRegenerate(regenerateCount)}</span>
          <div className="day-batch-confirm-actions">
            <button
              type="button"
              className="btn btn-primary btn-xs"
              onClick={() => {
                setConfirmRegenerate(false);
                void onRunBatch("regenerate");
              }}
            >
              {t.confirm}
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              onClick={() => setConfirmRegenerate(false)}
            >
              {tr.common.cancel}
            </button>
          </div>
        </div>
      )}

      <div className="day-batch-row">
        <div className="day-batch-split">
          <button
            type="button"
            className="btn btn-primary btn-xs day-batch-main-btn"
            disabled={selectedCount === 0 || planning || smartCount === 0}
            title={
              smartCount > 0
                ? t.smartTitle(missingCount, staleCount)
                : t.nothingToDo
            }
            onClick={() => void onRunBatch("smart")}
          >
            {t.generateUpdate}
          </button>
          <DropdownMenu
            label="▾"
            align="left"
            direction="up"
            variant="secondary"
            size="xs"
            hideChevron
            disabled={selectedCount === 0 || planning}
            buttonClassName="day-batch-caret"
            ariaLabel={t.options}
          >
            {(close) => (
              <div className="dropdown-form">
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={staleCount === 0}
                  onClick={() => {
                    close();
                    void onRunBatch("stale");
                  }}
                >
                  {t.updateStale(staleCount)}
                </button>
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={missingCount === 0}
                  onClick={() => {
                    close();
                    void onRunBatch("missing");
                  }}
                >
                  {t.generateMissing(missingCount)}
                </button>
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={regenerateCount === 0}
                  onClick={() => {
                    close();
                    setConfirmRegenerate(true);
                  }}
                >
                  {t.regenerateAll(regenerateCount)}
                </button>
                <div className="dropdown-sep" />
                <label className="day-batch-check">
                  <input
                    type="checkbox"
                    checked={includeFinal}
                    onChange={(e) => onChangeIncludeFinal(e.target.checked)}
                  />
                  <span>{t.includeFinal}</span>
                </label>
              </div>
            )}
          </DropdownMenu>
        </div>

        <div className="day-batch-status-btns">
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            disabled={selectedCount === 0 || planning}
            onClick={() => void onSetStatus("final")}
            title={t.finalTitle}
          >
            {t.final}
          </button>
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            disabled={selectedCount === 0 || planning}
            onClick={() => void onSetStatus("draft")}
            title={t.draftTitle}
          >
            {t.draft}
          </button>
        </div>
      </div>

      {batchSummary && <div className="day-batch-summary">{batchSummary}</div>}
    </div>
  );
}
