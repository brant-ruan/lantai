// Sidebar calendar: a day heatmap (daily view), the same month as ISO-week rows (weekly
// view), or a 4x3 month heatmap of one year (monthly view). The shown month and year are
// owned by the sidebar so its lists can move the calendar too.

import { useMemo } from "react";
import { monthName, useLanguage, weekdayInitials } from "../../i18n";
import type { DayOverview, MonthOverview, WeekOverview } from "../../types";
import {
  activityLevel,
  isoWeekKey,
  monthCells,
  shiftMonth,
  shiftYear,
  weekMonth,
} from "./dates";
import { useSwipeStep } from "./useSwipeStep";
import { TargetIcon } from "../icons";

export type CalendarMode = "daily" | "weekly" | "monthly";

interface SidebarCalendarProps {
  mode: CalendarMode;
  /** Shown month ("YYYY-MM") in day and week modes. */
  month: string;
  setMonth: (update: (prev: string) => string) => void;
  /** Shown year ("YYYY") in month mode. */
  year: string;
  setYear: (update: (prev: string) => string) => void;
  today: string;
  days: DayOverview[];
  selectedDate: string;
  onSelectDate: (date: string) => void;
  weeks: WeekOverview[];
  currentWeek: string;
  selectedWeek: string;
  onSelectWeek: (week: string) => void;
  months: MonthOverview[];
  currentMonth: string;
  selectedMonth: string;
  onSelectMonth: (month: string) => void;
}

/** Overviews keyed by period, plus the largest active time for the heat scale. */
function useHeat<T extends { activeMinutes: number }>(items: T[], key: (t: T) => string) {
  return useMemo(() => {
    const map = new Map<string, T>();
    for (const it of items) map.set(key(it), it);
    return { map, max: Math.max(60, ...items.map((it) => it.activeMinutes)) };
    // `key` is a constant accessor at each call site, so only `items` matters.
  }, [items]);
}

export function SidebarCalendar(props: SidebarCalendarProps) {
  const { mode, month, setMonth, year, setYear, today } = props;
  const { lang, t } = useLanguage();
  const tc = t.calendar;
  const weekdays = useMemo(() => weekdayInitials(lang), [lang]);
  const monthMode = mode === "monthly";
  const step = (delta: 1 | -1) =>
    monthMode ? setYear((y) => shiftYear(y, delta)) : setMonth((m) => shiftMonth(m, delta));
  const swipe = useSwipeStep(step);

  const dayHeat = useHeat(props.days, (d) => d.localDate);
  const weekHeat = useHeat(props.weeks, (w) => w.week);
  const monthHeat = useHeat(props.months, (m) => m.month);
  const cells = useMemo(() => monthCells(month), [month]);
  const todayMonth = today.slice(0, 7);
  const dayLevel = (date: string) =>
    activityLevel(dayHeat.map.get(date)?.activeMinutes ?? 0, dayHeat.max);

  const dayGrid = (
    <div className="calendar-grid">
      {weekdays.map((w, i) => (
        <div key={i} className="calendar-weekday">
          {w}
        </div>
      ))}
      {cells.map((cell) => {
        const info = dayHeat.map.get(cell.date);
        const classes = [
          "calendar-day",
          `level-${dayLevel(cell.date)}`,
          cell.inMonth ? "" : "outside-month",
          cell.date === props.selectedDate ? "selected" : "",
          cell.date === today ? "today" : "",
        ];
        return (
          <button
            key={cell.date}
            type="button"
            className={classes.join(" ")}
            title={
              info
                ? tc.daySummary(
                    cell.date,
                    info.slices,
                    info.activeMinutes,
                    info.unassignedSlices,
                  )
                : cell.date
            }
            onClick={() => props.onSelectDate(cell.date)}
          >
            <span>{cell.day}</span>
          </button>
        );
      })}
    </div>
  );

  // One row per ISO week with its number; dates are muted and the selected week is framed.
  const weekRows = Array.from({ length: cells.length / 7 }, (_, i) => cells.slice(i * 7, i * 7 + 7));
  const weekGrid = (
    <div className="calendar-week-grid">
      <div className="calendar-week-row header">
        <span className="calendar-weekday">{tc.week}</span>
        {weekdays.map((w, i) => (
          <span key={i} className="calendar-weekday">
            {w}
          </span>
        ))}
      </div>
      {weekRows.map((row) => {
        const key = isoWeekKey(row[0].date);
        const info = weekHeat.map.get(key);
        const level = activityLevel(info?.activeMinutes ?? 0, weekHeat.max);
        return (
          <button
            key={key}
            type="button"
            className={`calendar-week-row ${key === props.selectedWeek ? "selected" : ""} ${
              key === props.currentWeek ? "current" : ""
            }`}
            title={
              info
                ? tc.weekSummary(
                    key,
                    info.activeDays,
                    info.slices,
                    info.activeMinutes,
                    info.reportStatus,
                  )
                : key
            }
            onClick={() => props.onSelectWeek(key)}
          >
            <span className={`calendar-week-num level-${level}`}>W{key.slice(6)}</span>
            {row.map((cell) => (
              <span
                key={cell.date}
                className={`calendar-day level-${dayLevel(cell.date)} ${
                  cell.inMonth ? "" : "outside-month"
                }`}
              >
                {cell.day}
              </span>
            ))}
          </button>
        );
      })}
    </div>
  );

  const monthGrid = (
    <div className="calendar-month-grid">
      {Array.from({ length: 12 }, (_, idx) => {
        const label = monthName(lang, idx);
        const key = `${year}-${String(idx + 1).padStart(2, "0")}`;
        const info = monthHeat.map.get(key);
        const level = activityLevel(info?.activeMinutes ?? 0, monthHeat.max);
        return (
          <button
            key={key}
            type="button"
            className={`calendar-day calendar-month-cell level-${level} ${
              key === props.selectedMonth ? "selected" : ""
            } ${key === props.currentMonth ? "today" : ""}`}
            title={
              info
                ? tc.monthSummary(
                    key,
                    info.reportedWeeks,
                    info.activeWeeks,
                    info.slices,
                    info.activeMinutes,
                  )
                : key
            }
            onClick={() => props.onSelectMonth(key)}
          >
            <span>{label}</span>
          </button>
        );
      })}
    </div>
  );

  // "Jump to the current period" button, per mode.
  const jump =
    mode === "monthly"
      ? {
          label: tc.thisMonth,
          title: props.currentMonth ? tc.jumpTo(tc.thisMonth, props.currentMonth) : tc.thisMonth,
          current: props.currentMonth,
          disabled:
            !props.currentMonth ||
            (props.selectedMonth === props.currentMonth &&
              year === props.currentMonth.slice(0, 4)),
          go: () => {
            setYear(() => props.currentMonth.slice(0, 4));
            props.onSelectMonth(props.currentMonth);
          },
        }
      : mode === "weekly"
        ? {
            label: tc.thisWeek,
            title: props.currentWeek ? tc.jumpTo(tc.thisWeek, props.currentWeek) : tc.thisWeek,
            current: props.currentWeek,
            disabled:
              !props.currentWeek ||
              (props.selectedWeek === props.currentWeek &&
                month === weekMonth(props.currentWeek)),
            go: () => {
              setMonth(() => weekMonth(props.currentWeek) ?? todayMonth);
              props.onSelectWeek(props.currentWeek);
            },
          }
        : {
            label: tc.today,
            title: today ? tc.jumpTo(tc.today, today) : tc.jumpToToday,
            current: today,
            disabled: !today || (props.selectedDate === today && month === todayMonth),
            go: () => {
              setMonth(() => todayMonth);
              props.onSelectDate(today);
            },
          };

  const prevLabel = monthMode ? tc.previousYear : tc.previousMonth;
  const nextLabel = monthMode ? tc.nextYear : tc.nextMonth;
  return (
    <div
      className="sidebar-section calendar-swipe-area"
      onWheel={swipe.onWheel}
      onTouchStart={swipe.onTouchStart}
      onTouchEnd={swipe.onTouchEnd}
    >
      <div className="calendar-header">
        <span className="calendar-month">{monthMode ? year : month}</span>
        <div className="calendar-nav">
          <button
            type="button"
            className="btn-icon"
            onClick={() => step(-1)}
            title={prevLabel}
            aria-label={prevLabel}
          >
            &lt;
          </button>
          <button
            type="button"
            className="btn-icon calendar-today-btn"
            disabled={jump.disabled}
            onClick={() => {
              if (jump.current) jump.go();
            }}
            aria-label={jump.title}
            data-tip={jump.title}
            data-tip-align="end"
          >
            <TargetIcon />
          </button>
          <button
            type="button"
            className="btn-icon"
            onClick={() => step(1)}
            title={nextLabel}
            aria-label={nextLabel}
          >
            &gt;
          </button>
        </div>
      </div>
      {monthMode ? monthGrid : mode === "weekly" ? weekGrid : dayGrid}
    </div>
  );
}
