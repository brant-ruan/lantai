// The statistics dashboard's summary strip: active time, active days, sessions, files changed.

import type { StatsReport } from "../../types";
import { useT } from "../../i18n";
import { formatDuration, formatInt } from "./statsFormat";

export function StatsKpis({ stats }: { stats: StatsReport }) {
  const s = useT().stats;
  const avgPerDay =
    stats.activeDays > 0 ? Math.round(stats.activeMinutes / stats.activeDays) : 0;
  const avgTurnsPerSession =
    stats.sessions > 0 ? (stats.turns / stats.sessions).toFixed(1) : "0";

  return (
    <div className="stats-kpi-grid">
      <div className="stats-kpi-card">
        <div className="stats-kpi-label">{s.activeTime}</div>
        <div className="stats-kpi-value">
          {formatDuration(stats.activeMinutes)}
        </div>
        <div className="stats-kpi-sub">
          {stats.activeDays > 0
            ? s.perActiveDay(formatDuration(avgPerDay))
            : s.noActiveDays}
        </div>
      </div>

      <div className="stats-kpi-card">
        <div className="stats-kpi-label">{s.activeDays}</div>
        <div className="stats-kpi-value">
          {formatInt(stats.activeDays)}
          {stats.rangeDays > 0 && (
            <span className="stats-kpi-denom">
              {" "}
              / {formatInt(stats.rangeDays)}
            </span>
          )}
        </div>
        <div className="stats-kpi-sub">
          {stats.currentStreakDays > 0
            ? s.streak(stats.currentStreakDays, stats.longestStreakDays)
            : s.bestStreak(stats.longestStreakDays)}
        </div>
      </div>

      <div className="stats-kpi-card">
        <div className="stats-kpi-label">{s.sessions}</div>
        <div className="stats-kpi-value">
          {formatInt(stats.sessions)}
        </div>
        <div className="stats-kpi-sub">
          {s.turnsPerSession(formatInt(stats.turns), avgTurnsPerSession)}
        </div>
      </div>

      <div className="stats-kpi-card">
        <div className="stats-kpi-label">{s.filesChanged}</div>
        <div className="stats-kpi-value">
          {formatInt(stats.filesChanged)}
        </div>
        <div className="stats-kpi-sub stats-diff-line">
          <span className="stats-added">
            +{formatInt(stats.linesAdded)}
          </span>
          <span className="stats-removed">
            -{formatInt(stats.linesRemoved)}
          </span>
        </div>
      </div>
    </div>
  );
}
