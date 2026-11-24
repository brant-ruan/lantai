// Daily sidebar labels, stats and filters for the shared report list.

import type { ReportBatch } from "../../hooks/useReportBatch";
import { useLanguage, weekdayName } from "../../i18n";
import type { DayOverview, GenStage } from "../../types";
import { dayRowLabel, formatMinutes, isoWeekKey } from "./dates";
import { ReportList } from "./ReportList";

interface DayListProps {
  days: DayOverview[];
  today: string;
  currentWeek: string;
  selectedDate: string;
  generating: Record<string, GenStage>;
  onOpenDate: (date: string) => void;
  showIgnoredDays: boolean;
  onToggleShowIgnoredDays: () => void;
  escapeCloses: boolean;
  batch: ReportBatch;
}

export function DayList(props: DayListProps) {
  const { days, today, currentWeek } = props;
  const { lang, t } = useLanguage();
  const td = t.dayList;
  const rows = days.map((d) => {
    const ignored = d.slices === 0 && d.ignoredSlices > 0;
    return {
      key: d.localDate,
      slices: d.slices,
      reportStatus: d.reportStatus,
      ignored,
      label: (
        <span className="day-row-date" title={d.localDate}>
          {dayRowLabel(d.localDate, today)}
          <span className="day-row-weekday">
            {d.localDate === today ? td.today : weekdayName(lang, d.localDate)}
          </span>
        </span>
      ),
      meta: (
        <>
          {d.unassignedSlices > 0 && (
            <span className="badge badge-warn" title={t.sidebar.unassigned(d.unassignedSlices)}>
              {td.unassignedBadge(d.unassignedSlices)}
            </span>
          )}
          {d.notes > 0 && (
            <span className="day-row-notes" title={td.notesTitle(d.notes)}>{d.notes}</span>
          )}
          {(d.slices > 0 || d.ignoredSlices > 0) && (
            <span
              className="day-row-stats"
              title={ignored ? td.ignoredTitle(d.ignoredSlices) : td.statsTitle(d.slices, d.activeMinutes)}
            >
              {ignored ? td.ignored(d.ignoredSlices) : formatMinutes(d.activeMinutes)}
            </span>
          )}
        </>
      ),
    };
  });

  return (
    <ReportList
      kind="daily"
      title={td.days}
      rows={rows}
      current={props.selectedDate}
      generating={props.generating}
      onOpen={props.onOpenDate}
      batch={props.batch}
      escapeCloses={props.escapeCloses}
      filters={[
        { label: td.thisWeek, matches: (key) => isoWeekKey(key) === currentWeek },
        { label: td.thisMonth, matches: (key) => key.slice(0, 7) === today.slice(0, 7) },
      ]}
      extraOptions={(close) => (
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
        </>
      )}
    />
  );
}
