// Sidebar actions, monthly coverage preview and progress for a selected report batch.

import { useEffect, useState } from "react";
import { useT } from "../../i18n";
import type { ReportBatch } from "../../hooks/useReportBatch";
import type { PeriodMode } from "../../types";
import { DropdownMenu } from "../DropdownMenu";

export function ReportBatchBar({ batch, kind }: { batch: ReportBatch; kind: PeriodMode }) {
  const tr = useT();
  const t = tr.reportBatch;
  const { selectedKeys, plan, includeFinal, run, summary } = batch;
  const selectedCount = selectedKeys.length;
  const pending = batch.planning || batch.busy;
  const [confirmRegenerate, setConfirmRegenerate] = useState(false);

  useEffect(() => {
    setConfirmRegenerate(false);
  }, [selectedKeys, run, includeFinal]);

  const missingCount = plan?.missingCount ?? 0;
  const staleCount = plan?.staleCount ?? 0;
  const regenerateCount = plan?.regenerateCount ?? 0;
  const smartCount = missingCount + staleCount;
  const coverage = kind === "monthly"
    ? plan?.items.filter((item) => !item.hasInputs || item.missingWeeks.length > 0) ?? []
    : [];

  if (run !== null) {
    return (
      <div className="report-batch-bar">
        <div className="report-batch-row">
          <span className="batch-count">
            {t.running(run.done, run.total)}
          </span>
          <button
            type="button"
            className="btn btn-danger btn-xs"
            onClick={batch.cancel}
          >
            {t.cancelAll}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="report-batch-bar">
      <div className="report-batch-row">
        <span className="batch-count">
          {t.selected(selectedCount, kind)}
          {pending ? "…" : ""}
        </span>
        <button
          type="button"
          className="btn btn-ghost btn-xs report-batch-close"
          onClick={batch.close}
          disabled={batch.busy}
          title={t.exitTitle}
          aria-label={t.exit}
        >
          ×
        </button>
      </div>

      {confirmRegenerate && (
        <div className="report-batch-confirm">
          <span>{t.confirmRegenerate(regenerateCount)}</span>
          <div className="report-batch-confirm-actions">
            <button
              type="button"
              className="btn btn-primary btn-xs"
              disabled={pending || regenerateCount === 0}
              onClick={() => {
                setConfirmRegenerate(false);
                void batch.runBatch("regenerate");
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

      <div className="report-batch-row">
        <div className="report-batch-split">
          <button
            type="button"
            className="btn btn-primary btn-xs report-batch-main-btn"
            disabled={selectedCount === 0 || pending || smartCount === 0}
            title={
              smartCount > 0
                ? t.smartTitle(missingCount, staleCount)
                : t.nothingToDo
            }
            onClick={() => void batch.runBatch("smart")}
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
            disabled={selectedCount === 0 || pending}
            buttonClassName="report-batch-caret"
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
                    void batch.runBatch("stale");
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
                    void batch.runBatch("missing");
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
                <label className="report-batch-check">
                  <input
                    type="checkbox"
                    checked={includeFinal}
                    onChange={(e) => batch.setIncludeFinal(e.target.checked)}
                  />
                  <span>{t.includeFinal}</span>
                </label>
              </div>
            )}
          </DropdownMenu>
        </div>

        <div className="report-batch-status-btns">
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            disabled={selectedCount === 0 || pending}
            onClick={() => void batch.setStatus("final")}
            title={t.finalTitle}
          >
            {t.final}
          </button>
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            disabled={selectedCount === 0 || pending}
            onClick={() => void batch.setStatus("draft")}
            title={t.draftTitle}
          >
            {t.draft}
          </button>
        </div>
      </div>

      {coverage.length > 0 && (
        <ul className="report-batch-coverage">
          {coverage.map(({ periodKey, hasInputs, missingWeeks }) => (
            <li key={periodKey}>
              {hasInputs
                ? t.partialMonth(periodKey, missingWeeks.join(", "))
                : t.unavailableMonth(periodKey, missingWeeks.join(", "))}
            </li>
          ))}
        </ul>
      )}
      {summary && <div className="report-batch-summary" role="status">{summary}</div>}
    </div>
  );
}
