// The statistics dashboard's distribution: time by project on the left, by agent and tool
// category on the right. The project list shows as many rows as fit beside the right column
// (measured), with a toggle for the rest.

import { useLayoutEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { StatsReport } from "../../types";
import { useT } from "../../i18n";
import { formatDuration, formatInt, formatPct } from "./statsFormat";
import { SeriesSwatch, type SeriesLookup } from "./series";

interface StatsDistributionProps {
  stats: StatsReport;
  series: SeriesLookup;
  /** Shared with the trend chart, which highlights the same series. */
  hoveredSeriesKey: string | null;
  setHoveredSeriesKey: Dispatch<SetStateAction<string | null>>;
  onJumpToProject: (projectId: number) => void;
}

export function StatsDistribution({
  stats,
  series,
  hoveredSeriesKey,
  setHoveredSeriesKey,
  onJumpToProject,
}: StatsDistributionProps) {
  const s = useT().stats;
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

  // Adaptive project list cap, to stay flush with the right column.
  const allProjects = stats.projects;
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

  return (
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
            const info = series.resolve(p.projectId);
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
                    <SeriesSwatch info={info} round />
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
  );
}
