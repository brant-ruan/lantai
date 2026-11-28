// The statistics page: range and filter controls on top, then the dashboard's sections. The
// series colours and the hovered series are shared by the trend chart and the project list.

import { useLayoutEffect, useMemo, useState } from "react";
import { StatsDistribution } from "./StatsDistribution";
import { StatsKpis } from "./StatsKpis";
import { StatsLantai } from "./StatsLantai";
import { StatsTokens } from "./StatsTokens";
import { RhythmChart } from "./RhythmChart";
import { TrendChart } from "./TrendChart";
import { buildSeriesLookup } from "./series";
import type { LantaiStats, StatsRangeKey, StatsReport } from "../../types";
import { useT } from "../../i18n";

interface StatsViewProps {
  stats: StatsReport | null;
  loading: boolean;
  range: StatsRangeKey;
  includeIgnored: boolean;
  onChangeRange: (range: StatsRangeKey) => void;
  onToggleIncludeIgnored: (include: boolean) => void;
  onJumpToDay: (date: string) => void;
  onJumpToWeek: (week: string) => void;
  onJumpToMonth: (month: string) => void;
  onJumpToProject: (projectId: number) => void;
  lantai: LantaiStats | null;
  lantaiLoading: boolean;
  lantaiError: string | null;
}

const RANGES: StatsRangeKey[] = ["7d", "30d", "90d", "all"];

export function StatsView({
  stats,
  loading,
  range,
  includeIgnored,
  onChangeRange,
  onToggleIncludeIgnored,
  onJumpToDay,
  onJumpToWeek,
  onJumpToMonth,
  onJumpToProject,
  lantai,
  lantaiLoading,
  lantaiError,
}: StatsViewProps) {
  const s = useT().stats;
  const [hoveredSeriesKey, setHoveredSeriesKey] = useState<string | null>(null);

  useLayoutEffect(() => {
    setHoveredSeriesKey(null);
  }, [range, includeIgnored]);

  const series = useMemo(
    () => buildSeriesLookup(stats?.projects ?? [], { other: s.other, unassigned: s.unassigned }),
    [stats?.projects, s],
  );

  const dateSubtitle = stats?.sinceDate
    ? `${stats.sinceDate} ~ ${stats.untilDate}`
    : stats
      ? s.upTo(stats.untilDate)
      : "";
  // Changing the range or the filter starts the charts afresh (no stale hover, list collapsed);
  // each keyed sibling needs its own key.
  const resetKey = `${range}|${includeIgnored}`;

  return (
    <section className="panel stats-panel">
      <div className="panel-header">
        <div className="topbar-left">
          <h2>{s.title}</h2>
          {dateSubtitle && (
            <span className="muted stats-span-label">{dateSubtitle}</span>
          )}
          {loading && stats && <span className="muted">{s.refreshing}</span>}
        </div>
        <div className="topbar-right">
          <label
            className="stats-toggle"
            title={s.includeIgnoredTitle}
          >
            <input
              type="checkbox"
              checked={includeIgnored}
              onChange={(e) => onToggleIncludeIgnored(e.target.checked)}
            />
            <span>{s.includeIgnored}</span>
          </label>
          <div
            className="pill-group"
            role="tablist"
            aria-label={s.rangeLabel}
          >
            {RANGES.map((r) => (
              <button
                key={r}
                type="button"
                role="tab"
                aria-selected={range === r}
                className={`pill ${range === r ? "active" : ""}`}
                onClick={() => onChangeRange(r)}
              >
                {s.ranges[r]}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="stats-scroll">
        {!stats ? (
          <div className="empty-state">
            {loading ? s.loading : s.failed}
          </div>
        ) : stats.slices === 0 ? (
          <div className="empty-state">
            {s.empty}
          </div>
        ) : (
          <div className="stats-container">
            <StatsKpis stats={stats} />
            <TrendChart
              key={`trend|${resetKey}`}
              stats={stats}
              series={series}
              hoveredSeriesKey={hoveredSeriesKey}
              setHoveredSeriesKey={setHoveredSeriesKey}
              onJumpToDay={onJumpToDay}
              onJumpToWeek={onJumpToWeek}
            />
            <StatsDistribution
              key={`projects|${resetKey}`}
              stats={stats}
              series={series}
              hoveredSeriesKey={hoveredSeriesKey}
              setHoveredSeriesKey={setHoveredSeriesKey}
              onJumpToProject={onJumpToProject}
            />
            <RhythmChart stats={stats} />
            <StatsTokens stats={stats} />
            <StatsLantai
              data={lantai}
              loading={lantaiLoading}
              error={lantaiError}
              onJumpToDay={onJumpToDay}
              onJumpToWeek={onJumpToWeek}
              onJumpToMonth={onJumpToMonth}
            />
          </div>
        )}
      </div>
    </section>
  );
}
