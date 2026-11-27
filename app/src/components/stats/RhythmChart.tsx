// The statistics dashboard's 24-hour rhythm: active time per local hour, shaded by intensity,
// with the night hours banded.

import { useMemo, useState } from "react";
import type { StatsReport } from "../../types";
import { useT } from "../../i18n";
import { formatDuration, padHour } from "./statsFormat";

export function RhythmChart({ stats }: { stats: StatsReport }) {
  const s = useT().stats;
  const [hoveredHour, setHoveredHour] = useState<number | null>(null);
  const hourlyMax = useMemo(
    () => Math.max(1, ...stats.hourly.map((h) => h.activeMinutes)),
    [stats],
  );
  const hoveredHourBucket =
    hoveredHour !== null && stats.hourly[hoveredHour]
      ? stats.hourly[hoveredHour]
      : null;

  return (
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
  );
}
