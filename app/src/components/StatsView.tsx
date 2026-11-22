import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { StatsLantai } from "./StatsLantai";
import { StatsTokens } from "./StatsTokens";
import { formatInt, formatPct } from "./statsFormat";
import type { LantaiStats, StatsRangeKey, StatsReport } from "../types";
import { useLanguage, weekdayName } from "../i18n";

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

interface SeriesInfo {
  key: string;
  label: string;
  color: string;
  svgFill: string;
  isUnassigned: boolean;
  order: number;
}

const RANGES: StatsRangeKey[] = ["7d", "30d", "90d", "all"];

const SERIES_COLORS = [
  "var(--series-1)",
  "var(--series-2)",
  "var(--series-3)",
  "var(--series-4)",
  "var(--series-5)",
];

function formatDuration(mins: number): string {
  if (mins <= 0) return "0m";
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  const rem = mins % 60;
  if (h >= 10 || rem === 0) {
    const rounded = mins / 60;
    return `${h >= 10 ? Math.round(rounded) : rounded.toFixed(1).replace(/\.0$/, "")}h`;
  }
  return `${h}h ${rem}m`;
}

function formatAxisDuration(mins: number): string {
  if (mins <= 0) return "0";
  if (mins < 60) return `${mins}m`;
  const h = mins / 60;
  return `${h >= 10 ? Math.round(h) : h.toFixed(1).replace(/\.0$/, "")}h`;
}

function padHour(h: number): string {
  return String(h).padStart(2, "0");
}

function niceCeilMinutes(maxMins: number): number {
  if (maxMins <= 0) return 60;
  const candidates = [
    15, 30, 60, 120, 180, 240, 360, 480, 600, 720, 960, 1200, 1500, 1800, 2400,
    3000, 3600, 4800, 6000,
  ];
  for (const c of candidates) {
    if (c >= maxMins) return c;
  }
  const hours = Math.ceil(maxMins / 60);
  const step = Math.ceil(hours / 10) * 10;
  return step * 60;
}

/** Right-anchored X-axis ticks guaranteed not to overlap adjacent labels. */
function computeTickIndices(count: number, slotW: number): Set<number> {
  const set = new Set<number>();
  if (count <= 0) return set;
  if (count === 1) {
    set.add(0);
    return set;
  }
  const minGap = Math.max(1, Math.ceil(48 / Math.max(1, slotW)));
  const stride = Math.max(minGap, Math.ceil((count - 1) / 7));
  const last = count - 1;
  let smallest = last;
  for (let i = last; i >= 0; i -= stride) {
    set.add(i);
    smallest = i;
  }
  if (smallest >= minGap) {
    set.add(0);
  }
  return set;
}

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
  const { lang, t: tr } = useLanguage();
  const s = tr.stats;
  const [hoveredTrendIdx, setHoveredTrendIdx] = useState<number | null>(null);
  const [hoveredHour, setHoveredHour] = useState<number | null>(null);
  const [hoveredSeriesKey, setHoveredSeriesKey] = useState<string | null>(null);
  const [showAllProjects, setShowAllProjects] = useState(false);

  const rightContentRef = useRef<HTMLDivElement | null>(null);
  const leftHeadRef = useRef<HTMLDivElement | null>(null);
  const sampleRowRef = useRef<HTMLDivElement | null>(null);
  const [layoutMetrics, setLayoutMetrics] = useState({
    rightH: 0,
    headH: 24,
    rowPitch: 52,
  });

  useLayoutEffect(() => {
    setShowAllProjects(false);
    setHoveredSeriesKey(null);
    setHoveredTrendIdx(null);
  }, [range, includeIgnored]);

  useLayoutEffect(() => {
    const measure = () => {
      const rightH =
        rightContentRef.current?.getBoundingClientRect().height ?? 0;
      const headH = leftHeadRef.current?.getBoundingClientRect().height ?? 24;
      const rowH = sampleRowRef.current?.getBoundingClientRect().height;
      const rowPitch = rowH && rowH > 16 ? Math.round(rowH + 10) : 52;
      setLayoutMetrics((prev) => {
        if (
          Math.abs(prev.rightH - rightH) <= 1 &&
          Math.abs(prev.headH - headH) <= 1 &&
          Math.abs(prev.rowPitch - rowPitch) <= 1
        ) {
          return prev;
        }
        return { rightH, headH, rowPitch };
      });
    };
    measure();
    const ro = new ResizeObserver(measure);
    if (rightContentRef.current) ro.observe(rightContentRef.current);
    if (leftHeadRef.current) ro.observe(leftHeadRef.current);
    if (sampleRowRef.current) ro.observe(sampleRowRef.current);
    window.addEventListener("resize", measure);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [stats, showAllProjects]);

  const avgPerDay =
    stats && stats.activeDays > 0
      ? Math.round(stats.activeMinutes / stats.activeDays)
      : 0;
  const avgTurnsPerSession =
    stats && stats.sessions > 0
      ? (stats.turns / stats.sessions).toFixed(1)
      : "0";

  // Series grammar: color strictly encodes projects (Top 5 + Other + striped Unassigned).
  const seriesLookup = useMemo(() => {
    const topNamed = (stats?.projects ?? [])
      .filter((p) => p.projectId !== null && p.activeMinutes > 0)
      .slice(0, 5);
    const byId = new Map<number, SeriesInfo>();
    topNamed.forEach((p, idx) => {
      if (p.projectId === null) return;
      const color = SERIES_COLORS[idx];
      byId.set(p.projectId, {
        key: `p:${p.projectId}`,
        label: p.name,
        color,
        svgFill: color,
        isUnassigned: false,
        order: idx,
      });
    });

    const otherSeries: SeriesInfo = {
      key: "other",
      label: s.other,
      color: "var(--series-other)",
      svgFill: "var(--series-other)",
      isUnassigned: false,
      order: 5,
    };
    const unassignedSeries: SeriesInfo = {
      key: "unassigned",
      label: s.unassigned,
      color: "var(--series-unassigned-bg)",
      svgFill: "url(#stats-unassigned-pattern)",
      isUnassigned: true,
      order: 6,
    };

    const resolve = (projectId: number | null): SeriesInfo => {
      if (projectId === null) return unassignedSeries;
      return byId.get(projectId) ?? otherSeries;
    };

    const legendMap = new Map<string, SeriesInfo>();
    for (const p of stats?.projects ?? []) {
      if (p.activeMinutes <= 0) continue;
      const info = resolve(p.projectId);
      if (!legendMap.has(info.key)) legendMap.set(info.key, info);
    }
    const legend = Array.from(legendMap.values()).sort(
      (a, b) => a.order - b.order,
    );

    return { resolve, legend };
  }, [stats?.projects, s]);

  const trendScale = useMemo(() => {
    const rawMax = Math.max(
      0,
      ...(stats?.trend.map((b) => b.activeMinutes) ?? []),
    );
    const yMax = niceCeilMinutes(rawMax);
    const peak =
      stats?.trend.reduce(
        (best, cur) =>
          cur.activeMinutes > (best?.activeMinutes ?? 0) ? cur : best,
        null as StatsReport["trend"][number] | null,
      ) ?? null;
    return { yMax, peak };
  }, [stats]);

  const hourlyMax = useMemo(
    () => Math.max(1, ...(stats?.hourly.map((h) => h.activeMinutes) ?? [])),
    [stats],
  );

  // Compute adaptive project list cap to stay flush with the right column.
  const allProjects = stats?.projects ?? [];
  const availableListH =
    layoutMetrics.rightH > 0
      ? Math.max(160, Math.floor(layoutMetrics.rightH - layoutMetrics.headH - 12))
      : 380;
  const totalNaturalListH =
    allProjects.length > 0
      ? allProjects.length * layoutMetrics.rowPitch - 10
      : 0;
  const fitsAll = totalNaturalListH <= availableListH;
  const fitCount = Math.max(
    3,
    Math.floor((availableListH - 34 + 10) / layoutMetrics.rowPitch),
  );
  const visibleProjects =
    fitsAll || showAllProjects ? allProjects : allProjects.slice(0, fitCount);
  const listMaxHeight =
    !fitsAll && showAllProjects ? Math.max(150, availableListH - 34) : undefined;

  const dateSubtitle = stats?.sinceDate
    ? `${stats.sinceDate} ~ ${stats.untilDate}`
    : stats
      ? s.upTo(stats.untilDate)
      : "";

  const hoveredTrend =
    hoveredTrendIdx !== null && stats?.trend[hoveredTrendIdx]
      ? stats.trend[hoveredTrendIdx]
      : null;

  const hoveredHourBucket =
    hoveredHour !== null && stats?.hourly[hoveredHour]
      ? stats.hourly[hoveredHour]
      : null;

  // Trend SVG geometry
  const W = 760;
  const H = 188;
  const PL = 42;
  const PR = 18;
  const PT = 12;
  const PB = 26;
  const plotW = W - PL - PR;
  const plotH = H - PT - PB;
  const buckets = stats?.trend ?? [];
  const slotW = plotW / Math.max(1, buckets.length);
  const tickSet = useMemo(
    () => computeTickIndices(buckets.length, slotW),
    [buckets.length, slotW],
  );

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
            {/* Layer 1: KPI summary strip */}
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

            {/* Layer 2: Stacked-by-project Activity Trend */}
            <section className="stats-card">
              <div className="stats-card-head">
                <div className="stats-card-title">
                  <span>{s.activityTrend}</span>
                  <span className="badge">
                    {s.granularity(stats.trendGranularity === "day")}
                  </span>
                </div>
                {trendScale.peak && trendScale.peak.activeMinutes > 0 && (
                  <span className="muted stats-card-meta">
                    {s.peak(trendScale.peak.label, formatDuration(trendScale.peak.activeMinutes))}
                  </span>
                )}
              </div>

              {seriesLookup.legend.length > 0 && (
                <div className="stats-legend">
                  {seriesLookup.legend.map((s) => {
                    const hl = hoveredSeriesKey === s.key;
                    const dim =
                      hoveredSeriesKey !== null && hoveredSeriesKey !== s.key;
                    return (
                      <span
                        key={s.key}
                        className={`stats-legend-item ${hl ? "highlighted" : ""} ${dim ? "dimmed" : ""}`}
                        onMouseEnter={() => setHoveredSeriesKey(s.key)}
                        onMouseLeave={() =>
                          setHoveredSeriesKey((cur) =>
                            cur === s.key ? null : cur,
                          )
                        }
                      >
                        <span
                          className={`stats-swatch ${s.isUnassigned ? "unassigned" : ""}`}
                          style={s.isUnassigned ? undefined : { background: s.color }}
                        />
                        <span>{s.label}</span>
                      </span>
                    );
                  })}
                </div>
              )}

              <div className="stats-chart-wrap">
                {hoveredTrend && hoveredTrendIdx !== null && (
                  <div
                    className="stats-tooltip"
                    style={{
                      left: `${Math.min(
                        84,
                        Math.max(
                          16,
                          ((hoveredTrendIdx + 0.5) /
                            Math.max(1, buckets.length)) *
                            100,
                        ),
                      )}%`,
                    }}
                  >
                    <div className="stats-tooltip-title">
                      {stats.trendGranularity === "day"
                        ? `${hoveredTrend.key} (${weekdayName(lang, hoveredTrend.key)})`
                        : hoveredTrend.dateRange}
                    </div>
                    <div className="stats-tooltip-row">
                      {hoveredTrend.activeMinutes > 0 ? (
                        <>
                          <strong>
                            {formatDuration(hoveredTrend.activeMinutes)}
                          </strong>
                          <span>{s.sessionCount(hoveredTrend.sessions)}</span>
                        </>
                      ) : (
                        <span className="muted">{s.noActivity}</span>
                      )}
                    </div>
                    {hoveredTrend.segments.length > 0 && (
                      <div className="stats-tooltip-list">
                        {hoveredTrend.segments.slice(0, 4).map((seg) => {
                          const info = seriesLookup.resolve(seg.projectId);
                          return (
                            <div
                              key={`${seg.projectId ?? "u"}-${seg.name}`}
                              className="stats-tooltip-seg"
                            >
                              <span className="stats-tooltip-seg-name">
                                <span
                                  className={`stats-swatch ${info.isUnassigned ? "unassigned" : ""}`}
                                  style={
                                    info.isUnassigned
                                      ? undefined
                                      : { background: info.color }
                                  }
                                />
                                <span>{seg.projectId === null ? s.unassigned : seg.name}</span>
                              </span>
                              <span>{formatDuration(seg.activeMinutes)}</span>
                            </div>
                          );
                        })}
                        {hoveredTrend.segments.length > 4 && (
                          <div className="stats-tooltip-seg muted">
                            {s.moreProjects(hoveredTrend.segments.length - 4)}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                )}

                <svg
                  className="stats-trend-svg"
                  viewBox={`0 0 ${W} ${H}`}
                  role="img"
                  aria-label={s.trendAria}
                >
                  <defs>
                    <pattern
                      id="stats-unassigned-pattern"
                      width="6"
                      height="6"
                      patternUnits="userSpaceOnUse"
                      patternTransform="rotate(-45)"
                    >
                      <rect
                        width="6"
                        height="6"
                        fill="var(--series-unassigned-bg)"
                      />
                      <line
                        x1="0"
                        y1="0"
                        x2="0"
                        y2="6"
                        stroke="var(--series-unassigned-stripe)"
                        strokeWidth="2.4"
                      />
                    </pattern>
                    {buckets.map((b, i) => {
                      const ratio =
                        trendScale.yMax > 0
                          ? b.activeMinutes / trendScale.yMax
                          : 0;
                      const totalBarH =
                        b.activeMinutes > 0 ? Math.max(3, ratio * plotH) : 0;
                      if (totalBarH <= 0) return null;
                      const barW = Math.max(4, Math.min(28, slotW * 0.62));
                      const x = PL + i * slotW + (slotW - barW) / 2;
                      const yTop = PT + plotH - totalBarH;
                      return (
                        <clipPath key={b.key} id={`stats-bar-clip-${i}`}>
                          <rect
                            x={x}
                            y={yTop}
                            width={barW}
                            height={totalBarH}
                            rx={3}
                          />
                        </clipPath>
                      );
                    })}
                  </defs>

                  {[0, 0.5, 1].map((frac) => {
                    const y = PT + plotH * (1 - frac);
                    const val = Math.round(trendScale.yMax * frac);
                    return (
                      <g key={frac}>
                        <line
                          x1={PL}
                          x2={W - PR}
                          y1={y}
                          y2={y}
                          className="stats-grid-line"
                        />
                        <text
                          x={PL - 8}
                          y={y + 3.5}
                          textAnchor="end"
                          className="stats-axis-text"
                        >
                          {formatAxisDuration(val)}
                        </text>
                      </g>
                    );
                  })}

                  {buckets.map((b, i) => {
                    const barW = Math.max(4, Math.min(28, slotW * 0.62));
                    const x = PL + i * slotW + (slotW - barW) / 2;
                    const slotX = PL + i * slotW;
                    const ratio =
                      trendScale.yMax > 0
                        ? b.activeMinutes / trendScale.yMax
                        : 0;
                    const totalBarH =
                      b.activeMinutes > 0 ? Math.max(3, ratio * plotH) : 0;
                    const yTop = PT + plotH - totalBarH;
                    const hovered = hoveredTrendIdx === i;
                    const clickable = b.activeMinutes > 0;
                    const showLabel = tickSet.has(i);
                    const clipId = `stats-bar-clip-${i}`;

                    // Aggregate bucket segments into global legend order (largest project at bottom).
                    const stackedMap = new Map<
                      string,
                      { info: SeriesInfo; rawMins: number }
                    >();
                    for (const seg of b.segments) {
                      const info = seriesLookup.resolve(seg.projectId);
                      const prev = stackedMap.get(info.key);
                      if (prev) {
                        prev.rawMins += seg.activeMinutes;
                      } else {
                        stackedMap.set(info.key, {
                          info,
                          rawMins: seg.activeMinutes,
                        });
                      }
                    }
                    const layers = Array.from(stackedMap.values()).sort(
                      (a, b) => a.info.order - b.info.order,
                    );
                    const segSum = layers.reduce((s, l) => s + l.rawMins, 0);

                    const rects: Array<{
                      key: string;
                      y: number;
                      h: number;
                      fill: string;
                      dimmed: boolean;
                    }> = [];
                    if (totalBarH > 0 && segSum > 0) {
                      let curBottom = PT + plotH;
                      layers.forEach((layer, idx) => {
                        const isLast = idx === layers.length - 1;
                        const segH = isLast
                          ? Math.max(0, curBottom - yTop)
                          : totalBarH * (layer.rawMins / segSum);
                        const segY = isLast ? yTop : curBottom - segH;
                        rects.push({
                          key: layer.info.key,
                          y: segY,
                          h: Math.max(0.5, segH),
                          fill: layer.info.svgFill,
                          dimmed:
                            hoveredSeriesKey !== null &&
                            hoveredSeriesKey !== layer.info.key,
                        });
                        curBottom -= segH;
                      });
                    } else if (totalBarH > 0) {
                      rects.push({
                        key: "other",
                        y: yTop,
                        h: totalBarH,
                        fill: "var(--series-1)",
                        dimmed: false,
                      });
                    }

                    return (
                      <g
                        key={b.key}
                        className={clickable ? "stats-bar-clickable" : undefined}
                        onMouseEnter={() => setHoveredTrendIdx(i)}
                        onMouseLeave={() =>
                          setHoveredTrendIdx((cur) => (cur === i ? null : cur))
                        }
                        onClick={() => {
                          if (!clickable) return;
                          if (stats.trendGranularity === "day") {
                            onJumpToDay(b.key);
                          } else {
                            onJumpToWeek(b.key);
                          }
                        }}
                      >
                        <rect
                          x={slotX}
                          y={PT}
                          width={slotW}
                          height={plotH}
                          fill="transparent"
                        />
                        {hovered && (
                          <rect
                            x={slotX + 1}
                            y={PT}
                            width={Math.max(1, slotW - 2)}
                            height={plotH}
                            rx={4}
                            className="stats-slot-hover"
                          />
                        )}
                        {totalBarH > 0 ? (
                          <g clipPath={`url(#${clipId})`}>
                            {rects.map((r) => (
                              <rect
                                key={r.key}
                                x={x}
                                y={r.y}
                                width={barW}
                                height={r.h}
                                fill={r.fill}
                                className={`stats-seg ${r.dimmed ? "dimmed" : ""}`}
                              />
                            ))}
                          </g>
                        ) : (
                          <line
                            x1={x}
                            x2={x + barW}
                            y1={PT + plotH - 0.5}
                            y2={PT + plotH - 0.5}
                            className="stats-empty-tick"
                          />
                        )}
                        {showLabel && (
                          <text
                            x={PL + i * slotW + slotW / 2}
                            y={H - 8}
                            textAnchor="middle"
                            className="stats-axis-text"
                          >
                            {b.label}
                          </text>
                        )}
                      </g>
                    );
                  })}
                </svg>
              </div>
            </section>

            {/* Layer 3: Project & Agent / Tool distribution (adaptive aligned height) */}
            <div className="stats-split-grid">
              <section className="stats-card stats-project-card">
                <div ref={leftHeadRef} className="stats-card-head">
                  <div className="stats-card-title">
                    <span>{s.byProject}</span>
                    <span className="badge">{allProjects.length}</span>
                  </div>
                </div>
                <div
                  className={`stats-list stats-project-list ${listMaxHeight ? "scrollable" : ""}`}
                  style={
                    listMaxHeight
                      ? { maxHeight: `${listMaxHeight}px` }
                      : undefined
                  }
                >
                  {visibleProjects.map((p, i) => {
                    const info = seriesLookup.resolve(p.projectId);
                    const hl = hoveredSeriesKey === info.key;
                    const dim =
                      hoveredSeriesKey !== null &&
                      hoveredSeriesKey !== info.key;
                    return (
                      <div
                        key={`${p.projectId ?? "unassigned"}-${p.name}`}
                        ref={i === 0 ? sampleRowRef : undefined}
                        className={`stats-dist-row ${hl ? "highlighted" : ""} ${dim ? "dimmed" : ""}`}
                        onMouseEnter={() => setHoveredSeriesKey(info.key)}
                        onMouseLeave={() =>
                          setHoveredSeriesKey((cur) =>
                            cur === info.key ? null : cur,
                          )
                        }
                      >
                        <div className="stats-dist-top">
                          <span className="stats-proj-title">
                            <span
                              className={`stats-swatch round ${info.isUnassigned ? "unassigned" : ""}`}
                              style={
                                info.isUnassigned
                                  ? undefined
                                  : { background: info.color }
                              }
                            />
                            {p.projectId !== null ? (
                              <button
                                type="button"
                                className="stats-project-link"
                                onClick={() => onJumpToProject(p.projectId!)}
                                title={s.open(p.name)}
                              >
                                {p.name}
                              </button>
                            ) : (
                              <span className="stats-unassigned-name">
                                {s.unassigned}
                              </span>
                            )}
                          </span>
                          <span className="stats-dist-val">
                            {formatDuration(p.activeMinutes)} ·{" "}
                            {formatPct(p.sharePct)}
                          </span>
                        </div>
                        <div className="stats-bar-track">
                          <div
                            className={`stats-bar-fill ${info.isUnassigned ? "unassigned-fill" : ""}`}
                            style={{
                              width: `${p.activeMinutes > 0 ? Math.max(2, p.sharePct) : 0}%`,
                              ...(info.isUnassigned
                                ? {}
                                : { background: info.color }),
                            }}
                          />
                        </div>
                        <div className="stats-dist-sub">
                          {s.projectSub(p.activeDays, p.sessions, p.filesChanged)}
                        </div>
                      </div>
                    );
                  })}
                </div>
                {!fitsAll && (
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs stats-show-more"
                    onClick={() => setShowAllProjects((v) => !v)}
                  >
                    {showAllProjects
                      ? s.showFewer
                      : s.showAll(allProjects.length)}
                  </button>
                )}
              </section>

              <section className="stats-card">
                <div ref={rightContentRef} className="stats-right-content">
                  <div className="stats-card-head">
                    <div className="stats-card-title">
                      <span>{s.byAgent}</span>
                    </div>
                  </div>
                  <div className="stats-list">
                    {stats.agents.map((a) => (
                      <div key={a.agent} className="stats-dist-row">
                        <div className="stats-dist-top">
                          <span className="stats-agent-name">{a.name}</span>
                          <span className="stats-dist-val">
                            {formatDuration(a.activeMinutes)} ·{" "}
                            {formatPct(a.sharePct)}
                          </span>
                        </div>
                        <div className="stats-bar-track">
                          <div
                            className="stats-bar-fill agent-fill"
                            style={{
                              width: `${a.activeMinutes > 0 ? Math.max(2, a.sharePct) : 0}%`,
                            }}
                          />
                        </div>
                        <div className="stats-dist-sub">
                          {s.agentSub(a.sessions, formatInt(a.turns), a.filesChanged)}
                        </div>
                      </div>
                    ))}
                  </div>

                  <div className="stats-divider" />

                  <div className="stats-card-head">
                    <div className="stats-card-title">
                      <span>{s.toolProfile}</span>
                      <span className="badge">
                        {formatInt(stats.totalToolCalls)}
                      </span>
                    </div>
                  </div>
                  <div className="stats-list">
                    {stats.tools.map((t) => {
                      const failPct =
                        t.key === "shell" && t.calls > 0 && t.errors > 0
                          ? Math.round((t.errors / t.calls) * 100)
                          : null;
                      return (
                        <div key={t.key} className="stats-tool-row">
                          <div className="stats-dist-top">
                            <span className="stats-tool-label">
                              {s.tools[t.key] ?? t.label}
                              {failPct !== null && (
                                <span
                                  className="stats-err-pill"
                                  title={s.failedCommands(t.errors)}
                                >
                                  {s.failedPct(failPct)}
                                </span>
                              )}
                            </span>
                            <span className="stats-dist-val">
                              {formatInt(t.calls)} · {formatPct(t.sharePct)}
                            </span>
                          </div>
                          <div className="stats-bar-track compact">
                            <div
                              className="stats-bar-fill tool-fill"
                              style={{
                                width: `${t.calls > 0 ? Math.max(2, t.sharePct) : 0}%`,
                              }}
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              </section>
            </div>

            {/* Layer 4: 24-hour local work rhythm (monochrome intensity + spatial night bands) */}
            <section className="stats-card">
              <div className="stats-card-head">
                <div className="stats-card-title">
                  <span>{s.workRhythm}</span>
                  <span className="muted stats-card-meta">{s.localTime}</span>
                </div>
                <div className="stats-rhythm-head-right">
                  <span className="stats-night-key">
                    <span className="stats-night-swatch" />
                    <span>{s.night}</span>
                  </span>
                  {stats.peakHour !== null && (
                    <span className="muted stats-card-meta">
                      {s.rhythmPeak(padHour(stats.peakHour), Math.round(stats.lateNightPct))}
                    </span>
                  )}
                </div>
              </div>

              <div className="stats-rhythm-wrap">
                {hoveredHourBucket && (
                  <div
                    className="stats-tooltip stats-tooltip-rhythm"
                    style={{
                      left: `${Math.min(
                        86,
                        Math.max(
                          14,
                          ((hoveredHourBucket.hour + 0.5) / 24) * 100,
                        ),
                      )}%`,
                    }}
                  >
                    <div className="stats-tooltip-title">
                      {padHour(hoveredHourBucket.hour)}:00 –{" "}
                      {padHour(hoveredHourBucket.hour)}:59
                    </div>
                    <div className="stats-tooltip-row">
                      <strong>
                        {formatDuration(hoveredHourBucket.activeMinutes)}
                      </strong>
                    </div>
                  </div>
                )}

                <div
                  className="stats-rhythm-bars"
                  role="img"
                  aria-label={s.rhythmAria}
                >
                  {stats.hourly.map((h) => {
                    const isNight = h.hour <= 5 || h.hour >= 22;
                    const heightPct =
                      h.activeMinutes > 0
                        ? Math.max(6, (h.activeMinutes / hourlyMax) * 100)
                        : 0;
                    const level =
                      h.activeMinutes > 0
                        ? Math.min(4, Math.ceil((h.activeMinutes / hourlyMax) * 4))
                        : 0;
                    const hovered = hoveredHour === h.hour;
                    const colClasses = [
                      "stats-hour-col",
                      isNight ? "night-band" : "",
                      h.hour === 0 || h.hour === 22 ? "band-first" : "",
                      h.hour === 5 || h.hour === 23 ? "band-last" : "",
                      h.hour === 5 ? "band-divider-right" : "",
                      h.hour === 22 ? "band-divider-left" : "",
                      hovered ? "hovered" : "",
                    ]
                      .filter(Boolean)
                      .join(" ");

                    return (
                      <div
                        key={h.hour}
                        className={colClasses}
                        onMouseEnter={() => setHoveredHour(h.hour)}
                        onMouseLeave={() =>
                          setHoveredHour((cur) => (cur === h.hour ? null : cur))
                        }
                      >
                        <div className="stats-hour-slot">
                          {heightPct > 0 ? (
                            <div
                              className={`stats-hour-bar lvl-${level}`}
                              style={{ height: `${heightPct}%` }}
                            />
                          ) : (
                            <div className="stats-hour-empty" />
                          )}
                        </div>
                        <span className="stats-hour-tick">
                          {h.hour === 0 ||
                          h.hour === 4 ||
                          h.hour === 8 ||
                          h.hour === 12 ||
                          h.hour === 16 ||
                          h.hour === 20 ||
                          h.hour === 23
                            ? padHour(h.hour)
                            : ""}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </section>

            {/* Layer 5: token usage */}
            <StatsTokens stats={stats} />

            {/* Layer 6: Lantai's own reports and caches */}
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
