// The sidebar's list of active days, with multi-select (checkboxes, Cmd/Ctrl-click,
// Shift-click ranges, quick filters) feeding the day batch bar.

import { useEffect, useMemo, useRef } from "react";
import { useLanguage, weekdayName } from "../../i18n";
import type { BatchPlan, DayBatchMode, DayBatchRun, DayOverview, GenStage } from "../../types";
import { DayBatchBar } from "../DayBatchBar";
import { DropdownMenu } from "../DropdownMenu";
import { dayRowLabel, formatMinutes, isoWeekKey } from "./dates";
import { StatusDot } from "./StatusDot";
import { isTextInput, modalOpen } from "../../dom";

export interface DayListProps {
  days: DayOverview[];
  today: string;
  currentWeek: string;
  selectedDate: string;
  generating: Record<string, GenStage>;
  /** Called when a row is clicked normally; the sidebar also moves the calendar there. */
  onOpenDate: (date: string) => void;
  showIgnoredDays: boolean;
  onToggleShowIgnoredDays: () => void;
  /** Escape leaves multi-select only while the daily view itself is shown. */
  escapeCloses: boolean;
  selectMode: boolean;
  selectedDates: string[];
  onChangeSelectMode: (on: boolean) => void;
  onChangeSelectedDates: (dates: string[]) => void;
  onQuickSelectStale: () => Promise<void>;
  batchPlan: BatchPlan | null;
  batchPlanning: boolean;
  batchIncludeFinal: boolean;
  onChangeBatchIncludeFinal: (include: boolean) => void;
  batchRun: DayBatchRun | null;
  batchSummary: string | null;
  onRunBatch: (mode: DayBatchMode) => Promise<void>;
  onCancelBatch: () => void;
  onSetBatchStatus: (status: "draft" | "final") => Promise<void>;
  onCloseBatch: () => void;
}

export function DayList(props: DayListProps) {
  const { days, today, selectedDates, batchRun, selectMode } = props;
  const { lang, t } = useLanguage();
  const td = t.dayList;
  const busy = batchRun !== null;
  const selectedSet = useMemo(() => new Set(selectedDates), [selectedDates]);
  const lastClickedRef = useRef<string | null>(null);

  // Escape leaves multi-select, unless typing or a modal is open.
  const { onCloseBatch, escapeCloses } = props;
  useEffect(() => {
    if (!escapeCloses || !selectMode || busy) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || isTextInput(e.target)) return;
      if (modalOpen()) return;
      onCloseBatch();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [escapeCloses, selectMode, busy, onCloseBatch]);

  /** Select `next`, kept in list order. */
  const select = (next: Set<string>) => {
    props.onChangeSelectMode(true);
    props.onChangeSelectedDates(days.map((d) => d.localDate).filter((d) => next.has(d)));
  };

  const toggle = (date: string) => {
    const next = new Set(selectedDates);
    if (!next.delete(date)) next.add(date);
    select(next);
    lastClickedRef.current = date;
  };

  const rangeTo = (date: string) => {
    const anchor = lastClickedRef.current ?? (props.selectedDate || date);
    const i1 = days.findIndex((d) => d.localDate === anchor);
    const i2 = days.findIndex((d) => d.localDate === date);
    if (i1 === -1 || i2 === -1) {
      toggle(date);
      return;
    }
    const next = new Set(selectedDates);
    for (const d of days.slice(Math.min(i1, i2), Math.max(i1, i2) + 1)) next.add(d.localDate);
    select(next);
    lastClickedRef.current ??= anchor;
  };

  const quickSelect = (pick: (d: DayOverview) => boolean) => {
    props.onChangeSelectMode(true);
    props.onChangeSelectedDates(days.filter(pick).map((d) => d.localDate));
  };

  const thisMonth = today.slice(0, 7);
  const quickFilters: Array<{ label: string; pick: (d: DayOverview) => boolean }> = [
    {
      label: td.thisWeek,
      pick: (d) => d.slices > 0 && isoWeekKey(d.localDate) === props.currentWeek,
    },
    { label: td.thisMonth, pick: (d) => d.slices > 0 && d.localDate.slice(0, 7) === thisMonth },
    { label: td.allDraft, pick: (d) => d.reportStatus === "draft" },
  ];

  const menu = (
    <DropdownMenu
      label="⋯"
      align="right"
      variant="ghost"
      size="xs"
      hideChevron
      buttonClassName="days-menu-btn"
      title={td.options}
      ariaLabel={td.options}
    >
      {(close) => (
        <>
          <button
            type="button"
            className="dropdown-item dropdown-item-check"
            onClick={() => {
              props.onToggleShowIgnoredDays();
              close();
            }}
          >
            <span>{td.showIgnoredDays}</span>
            <span className="dropdown-check-mark">{props.showIgnoredDays ? "✓" : ""}</span>
          </button>
          <div className="dropdown-sep" />
          <button
            type="button"
            className="dropdown-item"
            disabled={busy}
            onClick={() => {
              if (selectMode) onCloseBatch();
              else props.onChangeSelectMode(true);
              close();
            }}
          >
            {selectMode ? td.exitMultiSelect : td.selectDays}
          </button>
          <div className="dropdown-sep" />
          <div className="dropdown-section-label">{td.quickSelect}</div>
          {quickFilters.map((f) => (
            <button
              key={f.label}
              type="button"
              className="dropdown-item"
              disabled={busy}
              onClick={() => {
                quickSelect(f.pick);
                close();
              }}
            >
              {f.label}
            </button>
          ))}
          <button
            type="button"
            className="dropdown-item"
            disabled={busy}
            onClick={() => {
              close();
              void props.onQuickSelectStale();
            }}
          >
            {td.allOutOfDate}
          </button>
          <button
            type="button"
            className="dropdown-item"
            disabled={busy}
            onClick={() => {
              quickSelect((d) => d.slices > 0 && d.reportStatus === null);
              close();
            }}
          >
            {td.allWithoutReport}
          </button>
        </>
      )}
    </DropdownMenu>
  );

  return (
    <>
      <div className="sidebar-section days-list-wrap">
        <div className="section-header-row">
          <div className="section-label">{td.days}</div>
          {menu}
        </div>
        <div className="days-list">
          {days.map((d) => {
            const ignoredOnly = d.slices === 0 && d.ignoredSlices > 0;
            const isChecked = selectedSet.has(d.localDate);
            const classes = [
              "day-row-wrap",
              d.localDate === props.selectedDate ? "active" : "",
              ignoredOnly ? "day-row-ignored" : "",
              isChecked ? "day-row-selected" : "",
            ];
            return (
              <div key={d.localDate} className={classes.join(" ")}>
                {selectMode && (
                  <input
                    type="checkbox"
                    className="day-row-check"
                    checked={isChecked}
                    disabled={busy}
                    aria-label={td.selectDay(d.localDate)}
                    onChange={() => toggle(d.localDate)}
                  />
                )}
                <button
                  type="button"
                  className="day-row"
                  onClick={(e) => {
                    if (e.metaKey || e.ctrlKey || e.shiftKey) {
                      e.preventDefault();
                      if (busy) return;
                      if (e.shiftKey && !(e.metaKey || e.ctrlKey)) rangeTo(d.localDate);
                      else toggle(d.localDate);
                      return;
                    }
                    lastClickedRef.current = d.localDate;
                    props.onOpenDate(d.localDate);
                  }}
                >
                  <span className="day-row-date" title={d.localDate}>
                    {dayRowLabel(d.localDate, today)}
                    <span className="day-row-weekday">
                      {d.localDate === today ? td.today : weekdayName(lang, d.localDate)}
                    </span>
                  </span>
                  <span className="day-row-meta">
                    {d.unassignedSlices > 0 && (
                      <span
                        className="badge badge-warn"
                        title={t.sidebar.unassigned(d.unassignedSlices)}
                      >
                        {td.unassignedBadge(d.unassignedSlices)}
                      </span>
                    )}
                    {d.notes > 0 && (
                      <span className="day-row-notes" title={td.notesTitle(d.notes)}>
                        {d.notes}
                      </span>
                    )}
                    {(d.slices > 0 || d.ignoredSlices > 0) && (
                      <span
                        className="day-row-stats"
                        title={
                          ignoredOnly
                            ? td.ignoredTitle(d.ignoredSlices)
                            : td.statsTitle(d.slices, d.activeMinutes)
                        }
                      >
                        {ignoredOnly
                          ? td.ignored(d.ignoredSlices)
                          : formatMinutes(d.activeMinutes)}
                      </span>
                    )}
                    <StatusDot
                      generating={props.generating[d.localDate]}
                      reportStatus={d.reportStatus}
                    />
                  </span>
                </button>
              </div>
            );
          })}
        </div>
      </div>
      {(selectMode || busy || props.batchSummary !== null) && (
        <DayBatchBar
          selectedCount={selectedDates.length}
          plan={props.batchPlan}
          planning={props.batchPlanning}
          includeFinal={props.batchIncludeFinal}
          onChangeIncludeFinal={props.onChangeBatchIncludeFinal}
          batchRun={batchRun}
          batchSummary={props.batchSummary}
          onRunBatch={props.onRunBatch}
          onCancelBatch={props.onCancelBatch}
          onSetStatus={props.onSetBatchStatus}
          onClose={onCloseBatch}
        />
      )}
    </>
  );
}
