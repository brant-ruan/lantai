// Shared sidebar report rows, multi-selection controls and batch actions.

import { useEffect, useRef, type ReactNode } from "react";
import { isTextInput, modalOpen } from "../../dom";
import type { ReportBatch } from "../../hooks/useReportBatch";
import { useT } from "../../i18n";
import type { GenStage, PeriodMode } from "../../types";
import { DropdownMenu } from "../DropdownMenu";
import { ReportBatchBar } from "./ReportBatchBar";
import { StatusDot } from "./StatusDot";

interface ReportListRow {
  key: string;
  slices: number;
  reportStatus: "draft" | "final" | null;
  ignored?: boolean;
  label: ReactNode;
  meta: ReactNode;
}

interface ReportListProps {
  kind: PeriodMode;
  title: string;
  emptyText?: string;
  rows: ReportListRow[];
  current: string;
  generating: Record<string, GenStage>;
  onOpen: (key: string) => void;
  batch: ReportBatch;
  escapeCloses: boolean;
  extraOptions?: (close: () => void) => ReactNode;
  filters?: { label: string; matches: (key: string) => boolean }[];
}

export function ReportList({
  kind, title, emptyText, rows, current, generating, onOpen,
  batch, escapeCloses, extraOptions, filters = [],
}: ReportListProps) {
  const t = useT().dayList;
  const { selectMode, selectedKeys, busy, close: closeBatch } = batch;
  const selected = new Set(selectedKeys);
  const lastClicked = useRef<string | null>(null);

  useEffect(() => {
    if (!escapeCloses || !selectMode || busy) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || modalOpen()) return;
      const checkbox = e.target instanceof HTMLInputElement && e.target.type === "checkbox";
      if (checkbox || !isTextInput(e.target)) closeBatch();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [escapeCloses, selectMode, busy, closeBatch]);

  const select = (keys: Set<string>) => {
    batch.changeSelection(rows.map((row) => row.key).filter((key) => keys.has(key)));
  };

  const toggle = (key: string) => {
    const keys = new Set(selected);
    if (!keys.delete(key)) keys.add(key);
    select(keys);
    lastClicked.current = key;
  };

  const selectRange = (key: string) => {
    const anchor = lastClicked.current ?? current;
    const start = rows.findIndex((row) => row.key === anchor);
    const end = rows.findIndex((row) => row.key === key);
    if (start < 0 || end < 0) {
      toggle(key);
      return;
    }
    const range = rows.slice(Math.min(start, end), Math.max(start, end) + 1);
    select(new Set([...selected, ...range.map((row) => row.key)]));
    lastClicked.current ??= anchor;
  };

  const quickFilters = [
    ...filters.map(({ label, matches }) => ({
      label, pick: (row: ReportListRow) => row.slices > 0 && matches(row.key),
    })),
    { label: t.allDraft, pick: (row: ReportListRow) => row.reportStatus === "draft" },
  ];

  return (
    <>
      <div className="sidebar-section days-list-wrap">
        <div className="section-header-row">
          <div className="section-label">{title}</div>
          <DropdownMenu
            label="⋯"
            align="right"
            variant="ghost"
            size="xs"
            hideChevron
            buttonClassName="days-menu-btn"
            title={t.options[kind]}
            ariaLabel={t.options[kind]}
          >
            {(close) => (
              <>
                {extraOptions?.(close)}
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={busy}
                  onClick={() => {
                    batch.changeSelectMode(!selectMode);
                    close();
                  }}
                >
                  {selectMode ? t.exitMultiSelect : t.selectPeriods[kind]}
                </button>
                <div className="dropdown-sep" />
                <div className="dropdown-section-label">{t.quickSelect}</div>
                {quickFilters.map(({ label, pick }) => (
                  <button
                    key={label}
                    type="button"
                    className="dropdown-item"
                    disabled={busy}
                    onClick={() => {
                      batch.changeSelection(rows.filter(pick).map((row) => row.key));
                      close();
                    }}
                  >
                    {label}
                  </button>
                ))}
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={busy}
                  onClick={() => {
                    close();
                    void batch.quickSelectStale();
                  }}
                >
                  {t.allOutOfDate}
                </button>
                <button
                  type="button"
                  className="dropdown-item"
                  disabled={busy}
                  onClick={() => {
                    batch.changeSelection(rows.filter((row) => row.slices > 0 && !row.reportStatus)
                      .map((row) => row.key));
                    close();
                  }}
                >
                  {t.allWithoutReport}
                </button>
              </>
            )}
          </DropdownMenu>
        </div>
        <div className="days-list">
          {rows.length === 0 && emptyText && <div className="sidebar-note">{emptyText}</div>}
          {rows.map(({ key, ignored, label, meta, reportStatus }) => {
            const checked = selected.has(key);
            const classes = [
              "day-row-wrap",
              current === key ? "active" : "",
              ignored ? "day-row-ignored" : "",
              checked ? "day-row-selected" : "",
            ];
            return (
              <div key={key} className={classes.join(" ")}>
                {selectMode && (
                  <input
                    type="checkbox"
                    className="day-row-check"
                    checked={checked}
                    disabled={busy}
                    aria-label={t.selectPeriod(key)}
                    onChange={() => toggle(key)}
                  />
                )}
                <button
                  type="button"
                  className="day-row"
                  onClick={(e) => {
                    if (e.metaKey || e.ctrlKey || e.shiftKey) {
                      e.preventDefault();
                      if (busy) return;
                      if (e.shiftKey && !(e.metaKey || e.ctrlKey)) selectRange(key);
                      else toggle(key);
                      return;
                    }
                    lastClicked.current = key;
                    onOpen(key);
                  }}
                >
                  {label}
                  <span className="day-row-meta">
                    {meta}
                    <StatusDot generating={generating[key]} reportStatus={reportStatus} />
                  </span>
                </button>
              </div>
            );
          })}
        </div>
      </div>
      {(selectMode || busy || batch.summary !== null) && <ReportBatchBar batch={batch} kind={kind} />}
    </>
  );
}
