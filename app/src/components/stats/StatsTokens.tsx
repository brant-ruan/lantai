// Token usage section of the statistics dashboard: totals and composition, the per-bucket
// trend (same buckets as the activity trend), and the breakdown by model.

import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { formatInt, formatPct, formatTokens } from "./statsFormat";
import type { StatsReport, TokenUsage } from "../../types";
import { useT } from "../../i18n";

type PartKey = "outputTokens" | "inputTokens" | "cacheWriteTokens" | "cacheReadTokens";

/** Stack order (bottom to top) and colors; a monochrome ramp so tokens never look like projects. */
const PARTS: { key: PartKey; cls: string }[] = [
  { key: "outputTokens", cls: "tok-output" },
  { key: "inputTokens", cls: "tok-input" },
  { key: "cacheWriteTokens", cls: "tok-cache-write" },
  { key: "cacheReadTokens", cls: "tok-cache-read" },
];

function partPct(u: TokenUsage, key: PartKey): number {
  return u.totalTokens > 0 ? (u[key] / u.totalTokens) * 100 : 0;
}

/** True when nothing more is hidden below; the list then drops its bottom fade. */
function scrolledToEnd(el: HTMLElement): boolean {
  return el.scrollTop + el.clientHeight >= el.scrollHeight - 2;
}

/** Cache reads usually dwarf everything else; the trend can leave them out to show real work. */
function trendTotal(u: TokenUsage, withCacheRead: boolean): number {
  return withCacheRead ? u.totalTokens : u.totalTokens - u.cacheReadTokens;
}

export function StatsTokens({ stats }: { stats: StatsReport }) {
  const t = useT().tokens;
  const tokens = stats.tokens;
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [modelsAtEnd, setModelsAtEnd] = useState(false);
  const modelScroll = useRef<HTMLDivElement>(null);
  const [withCacheRead, setWithCacheRead] = useState(false);
  const trendParts = withCacheRead ? PARTS : PARTS.filter((p) => p.key !== "cacheReadTokens");

  const trendMax = useMemo(
    () => Math.max(1, ...stats.trend.map((b) => trendTotal(b.tokens, withCacheRead))),
    [stats.trend, withCacheRead],
  );

  // The list's height follows the chart beside it, so recheck the fade whenever either changes.
  useLayoutEffect(() => {
    const el = modelScroll.current;
    if (!el) return;
    const check = () => setModelsAtEnd(scrolledToEnd(el));
    check();
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => observer.disconnect();
  }, [tokens.models]);

  if (tokens.calls === 0) {
    return (
      <section className="stats-card">
        <div className="stats-card-head">
          <div className="stats-card-title">
            <span>{t.title}</span>
          </div>
        </div>
        <div className="muted stats-token-empty">{t.empty}</div>
      </section>
    );
  }

  const u = tokens.usage;
  const hovered = hoveredIdx !== null ? stats.trend[hoveredIdx] : null;

  return (
    <section className="stats-card">
      <div className="stats-card-head">
        <div className="stats-card-title">
          <span>{t.title}</span>
          <span className="muted stats-card-meta">{t.calls(formatInt(tokens.calls))}</span>
        </div>
        <div className="stats-legend">
          {PARTS.map((p) => (
            <span key={p.key} className="stats-legend-item">
              <span className={`stats-swatch ${p.cls}`} />
              <span>{t.parts[p.key]}</span>
            </span>
          ))}
        </div>
      </div>

      <div className="stats-token-figures">
        <div className="stats-token-figure">
          <div className="stats-kpi-label">{t.total}</div>
          <div className="stats-token-value">{formatTokens(u.totalTokens)}</div>
        </div>
        <div className="stats-token-figure">
          <div className="stats-kpi-label">{t.parts.outputTokens}</div>
          <div className="stats-token-value">{formatTokens(u.outputTokens)}</div>
        </div>
        <div className="stats-token-figure">
          <div className="stats-kpi-label">{t.parts.inputTokens}</div>
          <div className="stats-token-value">{formatTokens(u.inputTokens)}</div>
          <div className="stats-dist-sub">{t.cacheWriteSub(formatTokens(u.cacheWriteTokens))}</div>
        </div>
        <div className="stats-token-figure">
          <div className="stats-kpi-label">{t.parts.cacheReadTokens}</div>
          <div className="stats-token-value">{formatTokens(u.cacheReadTokens)}</div>
          <div className="stats-dist-sub">{t.cacheHitSub(formatPct(tokens.cacheHitPct))}</div>
        </div>
      </div>

      <div className="stats-token-composition" role="img" aria-label={t.composition}>
        {PARTS.map((p) => {
          const pct = partPct(u, p.key);
          return pct > 0 ? (
            <div
              key={p.key}
              className={`stats-token-part ${p.cls}`}
              style={{ width: `${pct}%` }}
              title={`${t.parts[p.key]}: ${formatInt(u[p.key])} (${formatPct(pct)})`}
            />
          ) : null;
        })}
      </div>

      <div className="stats-token-split">
        <div className="stats-token-trend-wrap">
          <div className="stats-token-subhead muted">
            <span>{t.perBucket(stats.trendGranularity === "week")}</span>
            <label className="stats-toggle">
              <input
                type="checkbox"
                checked={withCacheRead}
                onChange={(e) => setWithCacheRead(e.target.checked)}
              />
              {t.includeCacheReads}
            </label>
          </div>
          {hovered && hoveredIdx !== null && (
            <div
              className="stats-tooltip stats-tooltip-rhythm"
              style={{
                left: `${Math.min(
                  86,
                  Math.max(14, ((hoveredIdx + 0.5) / stats.trend.length) * 100),
                )}%`,
              }}
            >
              <div className="stats-tooltip-title">{hovered.dateRange}</div>
              <div className="stats-tooltip-row">
                <strong>{formatTokens(hovered.tokens.totalTokens)}</strong>
                <span className="muted">{t.tokensUnit}</span>
              </div>
              {hovered.tokens.totalTokens > 0 &&
                PARTS.map((p) => (
                  <div key={p.key} className="stats-tooltip-seg">
                    <span className={`stats-swatch ${p.cls}`} />
                    <span className="stats-tooltip-seg-name">{t.parts[p.key]}</span>
                    <span>{formatTokens(hovered.tokens[p.key])}</span>
                  </div>
                ))}
            </div>
          )}
          <div className="stats-token-bars">
            {stats.trend.map((b, i) => {
              const total = trendTotal(b.tokens, withCacheRead);
              const heightPct = total > 0 ? Math.max(3, (total / trendMax) * 100) : 0;
              return (
                <div
                  key={b.key}
                  className={`stats-token-col${hoveredIdx === i ? " hovered" : ""}`}
                  onMouseEnter={() => setHoveredIdx(i)}
                  onMouseLeave={() => setHoveredIdx((cur) => (cur === i ? null : cur))}
                >
                  <div className="stats-token-slot">
                    {total > 0 ? (
                      <div className="stats-token-stack" style={{ height: `${heightPct}%` }}>
                        {trendParts.map((p) => {
                          const pct = (b.tokens[p.key] / total) * 100;
                          return pct > 0 ? (
                            <div
                              key={p.key}
                              className={`stats-token-part ${p.cls}`}
                              style={{ height: `${pct}%` }}
                            />
                          ) : null;
                        })}
                      </div>
                    ) : (
                      <div className="stats-hour-empty" />
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <div className="stats-token-models">
          <div className="stats-token-subhead muted">{t.byModel}</div>
          <div className="stats-token-model-frame">
            <div
              className={`stats-list stats-token-model-scroll${modelsAtEnd ? " at-end" : ""}`}
              ref={modelScroll}
              onScroll={(e) => setModelsAtEnd(scrolledToEnd(e.currentTarget))}
            >
              {tokens.models.map((m) => (
                <div key={`${m.agent}|${m.model ?? ""}`} className="stats-dist-row">
                  <div className="stats-dist-top">
                    <span className="stats-agent-name">
                      {m.model ?? <span className="muted">{t.unknownModel}</span>}
                    </span>
                    <span className="stats-dist-val">
                      {formatTokens(m.usage.totalTokens)} · {formatPct(m.sharePct)}
                    </span>
                  </div>
                  <div className="stats-bar-track compact">
                    <div
                      className="stats-bar-fill agent-fill"
                      style={{ width: `${Math.max(2, m.sharePct)}%` }}
                    />
                  </div>
                  <div className="stats-dist-sub">
                    {t.modelSub(m.agentName, formatInt(m.calls), formatTokens(m.usage.outputTokens))}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
