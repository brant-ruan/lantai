// Weekly and monthly labels and activity counts for the shared sidebar report list.

import type { ReportBatch } from "../../hooks/useReportBatch";
import { useT } from "../../i18n";
import type { GenStage, MonthOverview, WeekOverview } from "../../types";
import { formatMinutes } from "./dates";
import { ReportList } from "./ReportList";

interface ListProps {
  current: string;
  selected: string;
  generating: Record<string, GenStage>;
  onOpen: (key: string) => void;
  batch: ReportBatch;
  escapeCloses: boolean;
}

function UnassignedBadge({ count }: { count: number }) {
  const t = useT();
  return count > 0 ? (
    <span className="badge badge-warn" title={t.sidebar.unassigned(count)}>{count}</span>
  ) : null;
}

export function WeekList({ weeks, current, selected, ...props }: ListProps & { weeks: WeekOverview[] }) {
  const t = useT();
  const rows = weeks.map((w) => ({
    key: w.week,
    slices: w.slices,
    reportStatus: w.reportStatus,
    label: (
      <span className="day-row-date" title={`${w.monday} ~ ${w.sunday}`}>
        {w.week.slice(5)}
        <span className="day-row-weekday">
          {w.week === current ? t.sidebar.thisWeek : `${w.monday.slice(5)} ~ ${w.sunday.slice(5)}`}
        </span>
      </span>
    ),
    meta: (
      <>
        <UnassignedBadge count={w.unassignedSlices} />
        <span
          className="day-row-stats"
          title={t.sidebar.weekStatsTitle(w.activeDays, w.slices, w.activeMinutes)}
        >
          {t.sidebar.weekStats(w.activeDays, formatMinutes(w.activeMinutes))}
        </span>
      </>
    ),
  }));
  return (
    <ReportList
      {...props}
      kind="weekly"
      title={t.dayList.weeks}
      current={selected}
      emptyText={t.sidebar.noWeeks}
      rows={rows}
    />
  );
}

export function MonthList({ months, current, selected, ...props }: ListProps & { months: MonthOverview[] }) {
  const t = useT();
  const rows = months.map((m) => ({
    key: m.month,
    slices: m.slices,
    reportStatus: m.reportStatus,
    label: (
      <span className="day-row-date" title={`${m.firstWeek} ~ ${m.lastWeek}`}>
        {m.month}
        <span className="day-row-weekday">
          {m.month === current ? t.sidebar.thisMonth : `${m.firstWeek} ~ ${m.lastWeek}`}
        </span>
      </span>
    ),
    meta: (
      <>
        <UnassignedBadge count={m.unassignedSlices} />
        <span
          className="day-row-stats"
          title={t.sidebar.monthStatsTitle(
            m.reportedWeeks, m.activeWeeks, m.activeDays, m.slices, m.activeMinutes,
          )}
        >
          {t.sidebar.monthStats(m.reportedWeeks, m.activeWeeks, formatMinutes(m.activeMinutes))}
        </span>
      </>
    ),
  }));
  return (
    <ReportList
      {...props}
      kind="monthly"
      title={t.dayList.months}
      current={selected}
      emptyText={t.sidebar.noMonths}
      rows={rows}
    />
  );
}
