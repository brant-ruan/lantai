// The statistics dashboard's activity trend: one bar per day or week, stacked by project in the
// series order, with a legend and a hover tooltip. Clicking a bar opens that day or week.

import { useMemo, useState, type Dispatch, type SetStateAction } from "react";
import type { StatsReport, StatsTrendSegment } from "../../types";
import { useLanguage, weekdayName } from "../../i18n";
import { formatAxisDuration, formatDuration } from "./statsFormat";
import { SeriesSwatch, UNASSIGNED_PATTERN_ID, type SeriesInfo, type SeriesLookup } from "./series";

// SVG geometry: view box and plot padding.
const W = 760;
const H = 188;
const PL = 42;
const PR = 18;
const PT = 12;
const PB = 26;
const PLOT_W = W - PL - PR;
const PLOT_H = H - PT - PB;

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

/** Where bucket `i`'s bar sits; a bar with any activity is at least 3 units tall. */
function barGeometry(activeMinutes: number, i: number, slotW: number, yMax: number) {
  const ratio = yMax > 0 ? activeMinutes / yMax : 0;
  const totalBarH = activeMinutes > 0 ? Math.max(3, ratio * PLOT_H) : 0;
  const barW = Math.max(4, Math.min(28, slotW * 0.62));
  const x = PL + i * slotW + (slotW - barW) / 2;
  const yTop = PT + PLOT_H - totalBarH;
  return { x, barW, totalBarH, yTop };
}

interface BarRect {
  key: string;
  y: number;
  h: number;
  fill: string;
  dimmed: boolean;
}

/**
 * One bar's segments, merged per series and stacked in legend order (largest project at the
 * bottom). The top segment absorbs rounding so the stack meets the bar's top exactly.
 */
function stackBar(
  segments: StatsTrendSegment[],
  series: SeriesLookup,
  yTop: number,
  totalBarH: number,
  hoveredSeriesKey: string | null,
): BarRect[] {
  const stackedMap = new Map<string, { info: SeriesInfo; rawMins: number }>();
  for (const seg of segments) {
    const info = series.resolve(seg.projectId);
    const prev = stackedMap.get(info.key);
    if (prev) {
      prev.rawMins += seg.activeMinutes;
    } else {
      stackedMap.set(info.key, { info, rawMins: seg.activeMinutes });
    }
  }
  const layers = Array.from(stackedMap.values()).sort((a, b) => a.info.order - b.info.order);
  const segSum = layers.reduce((s, l) => s + l.rawMins, 0);

  const rects: BarRect[] = [];
  if (totalBarH > 0 && segSum > 0) {
    let curBottom = PT + PLOT_H;
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
        dimmed: hoveredSeriesKey !== null && hoveredSeriesKey !== layer.info.key,
      });
      curBottom -= segH;
    });
  } else if (totalBarH > 0) {
    rects.push({ key: "other", y: yTop, h: totalBarH, fill: "var(--series-1)", dimmed: false });
  }
  return rects;
}

interface TrendChartProps {
  stats: StatsReport;
  series: SeriesLookup;
  /** Shared with the project list, which highlights the same series. */
  hoveredSeriesKey: string | null;
  setHoveredSeriesKey: Dispatch<SetStateAction<string | null>>;
  onJumpToDay: (date: string) => void;
  onJumpToWeek: (week: string) => void;
}

export function TrendChart({
  stats,
  series,
  hoveredSeriesKey,
  setHoveredSeriesKey,
  onJumpToDay,
  onJumpToWeek,
}: TrendChartProps) {
  const { lang, t } = useLanguage();
  const s = t.stats;
  const [hoveredTrendIdx, setHoveredTrendIdx] = useState<number | null>(null);

  const trendScale = useMemo(() => {
    const rawMax = Math.max(0, ...stats.trend.map((b) => b.activeMinutes));
    const yMax = niceCeilMinutes(rawMax);
    const peak = stats.trend.reduce(
      (best, cur) => (cur.activeMinutes > (best?.activeMinutes ?? 0) ? cur : best),
      null as StatsReport["trend"][number] | null,
    );
    return { yMax, peak };
  }, [stats]);

  const buckets = stats.trend;
  const slotW = PLOT_W / Math.max(1, buckets.length);
  const tickSet = useMemo(
    () => computeTickIndices(buckets.length, slotW),
    [buckets.length, slotW],
  );
  const hoveredTrend =
    hoveredTrendIdx !== null && stats.trend[hoveredTrendIdx]
      ? stats.trend[hoveredTrendIdx]
      : null;

  return (
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

      {series.legend.length > 0 && (
        <div className="stats-legend">
          {series.legend.map((item) => {
            const hl = hoveredSeriesKey === item.key;
            const dim =
              hoveredSeriesKey !== null && hoveredSeriesKey !== item.key;
            return (
              <span
                key={item.key}
                className={`stats-legend-item ${hl ? "highlighted" : ""} ${dim ? "dimmed" : ""}`}
                onMouseEnter={() => setHoveredSeriesKey(item.key)}
                onMouseLeave={() =>
                  setHoveredSeriesKey((cur) =>
                    cur === item.key ? null : cur,
                  )
                }
              >
                <SeriesSwatch info={item} />
                <span>{item.label}</span>
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
                  const info = series.resolve(seg.projectId);
                  return (
                    <div
                      key={`${seg.projectId ?? "u"}-${seg.name}`}
                      className="stats-tooltip-seg"
                    >
                      <span className="stats-tooltip-seg-name">
                        <SeriesSwatch info={info} />
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
              id={UNASSIGNED_PATTERN_ID}
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
              const { x, barW, totalBarH, yTop } = barGeometry(b.activeMinutes, i, slotW, trendScale.yMax);
              if (totalBarH <= 0) return null;
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
            const y = PT + PLOT_H * (1 - frac);
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
            const { x, barW, totalBarH, yTop } = barGeometry(b.activeMinutes, i, slotW, trendScale.yMax);
            const slotX = PL + i * slotW;
            const hovered = hoveredTrendIdx === i;
            const clickable = b.activeMinutes > 0;
            const showLabel = tickSet.has(i);
            const clipId = `stats-bar-clip-${i}`;
            const rects = stackBar(b.segments, series, yTop, totalBarH, hoveredSeriesKey);

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
                  height={PLOT_H}
                  fill="transparent"
                />
                {hovered && (
                  <rect
                    x={slotX + 1}
                    y={PT}
                    width={Math.max(1, slotW - 2)}
                    height={PLOT_H}
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
                    y1={PT + PLOT_H - 0.5}
                    y2={PT + PLOT_H - 0.5}
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
  );
}
