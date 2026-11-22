// Sidebar lists of active weeks and months, newest first.

import { useT } from "../../i18n";
import type { GenStage, MonthOverview, WeekOverview } from "../../types";
import { formatMinutes } from "./dates";
import { StatusDot } from "./StatusDot";

function UnassignedBadge({ count }: { count: number }) {
  const t = useT();
  return count > 0 ? (
    <span className="badge badge-warn" title={t.sidebar.unassigned(count)}>
      {count}
    </span>
  ) : null;
}

export function WeekList({
  weeks,
  currentWeek,
  selectedWeek,
  generating,
  onOpenWeek,
}: {
  weeks: WeekOverview[];
  currentWeek: string;
  selectedWeek: string;
  generating: Record<string, GenStage>;
  onOpenWeek: (week: string) => void;
}) {
  const t = useT();
  return (
    <div className="sidebar-section days-list-wrap">
      <div className="days-list">
        {weeks.length === 0 && <div className="sidebar-note">{t.sidebar.noWeeks}</div>}
        {weeks.map((w) => (
          <button
            key={w.week}
            type="button"
            className={`day-row ${w.week === selectedWeek ? "active" : ""}`}
            onClick={() => onOpenWeek(w.week)}
          >
            <span className="day-row-date" title={`${w.monday} ~ ${w.sunday}`}>
              {w.week.slice(5)}
              <span className="day-row-weekday">
                {w.week === currentWeek
                  ? t.sidebar.thisWeek
                  : `${w.monday.slice(5)} ~ ${w.sunday.slice(5)}`}
              </span>
            </span>
            <span className="day-row-meta">
              <UnassignedBadge count={w.unassignedSlices} />
              <span
                className="day-row-stats"
                title={t.sidebar.weekStatsTitle(w.activeDays, w.slices, w.activeMinutes)}
              >
                {t.sidebar.weekStats(w.activeDays, formatMinutes(w.activeMinutes))}
              </span>
              <StatusDot generating={generating[w.week]} reportStatus={w.reportStatus} />
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

export function MonthList({
  months,
  currentMonth,
  selectedMonth,
  generating,
  onOpenMonth,
}: {
  months: MonthOverview[];
  currentMonth: string;
  selectedMonth: string;
  generating: Record<string, GenStage>;
  onOpenMonth: (month: string) => void;
}) {
  const t = useT();
  return (
    <div className="sidebar-section days-list-wrap">
      <div className="days-list">
        {months.length === 0 && <div className="sidebar-note">{t.sidebar.noMonths}</div>}
        {months.map((m) => (
          <button
            key={m.month}
            type="button"
            className={`day-row ${m.month === selectedMonth ? "active" : ""}`}
            onClick={() => onOpenMonth(m.month)}
          >
            <span className="day-row-date" title={`${m.firstWeek} ~ ${m.lastWeek}`}>
              {m.month}
              <span className="day-row-weekday">
                {m.month === currentMonth ? t.sidebar.thisMonth : `${m.firstWeek} ~ ${m.lastWeek}`}
              </span>
            </span>
            <span className="day-row-meta">
              <UnassignedBadge count={m.unassignedSlices} />
              <span
                className="day-row-stats"
                title={t.sidebar.monthStatsTitle(
                  m.reportedWeeks,
                  m.activeWeeks,
                  m.activeDays,
                  m.slices,
                  m.activeMinutes,
                )}
              >
                {t.sidebar.monthStats(m.reportedWeeks, m.activeWeeks, formatMinutes(m.activeMinutes))}
              </span>
              <StatusDot generating={generating[m.month]} reportStatus={m.reportStatus} />
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
