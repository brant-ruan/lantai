// Lantai section of the statistics dashboard: the app's own state rather than the agents'.
// One timeline lines up month, week and day reports with each day's digest cache, so a
// column always means the same day; agents' digest coverage sits underneath.

import { formatInt, formatPct } from "./statsFormat";
import type {
  LantaiDigestTally,
  LantaiKindTotals,
  LantaiPeriodCell,
  LantaiStats,
} from "../../types";
import { useLanguage, useT } from "../../i18n";
import type { Messages } from "../../locales/en";

interface StatsLantaiProps {
  data: LantaiStats | null;
  loading: boolean;
  error: string | null;
  onJumpToDay: (date: string) => void;
  onJumpToWeek: (week: string) => void;
  onJumpToMonth: (month: string) => void;
}

type Texts = Messages["lantai"];

type DigestPart = "fresh" | "behind" | "missing";

const DIGEST_PARTS: DigestPart[] = ["fresh", "behind", "missing"];

const DAY_MS = 86_400_000;

function dayIndex(date: string, from: number): number {
  const [y, m, d] = date.split("-").map(Number);
  return Math.round((Date.UTC(y, m - 1, d) - from) / DAY_MS);
}

function pct(part: number, whole: number): number {
  return whole > 0 ? (part / whole) * 100 : 0;
}

function active(t: LantaiDigestTally): number {
  return t.fresh + t.behind + t.missing;
}

function tallyTitle(tally: LantaiDigestTally, t: Texts): string {
  return DIGEST_PARTS.map((p) => `${t.digest[p]} ${formatInt(tally[p])}`).join(" · ");
}

function cellTitle(c: LantaiPeriodCell, t: Texts): string {
  const parts = [c.key, t.status[c.status]];
  if (c.versions > 1) parts.push(t.versions(c.versions));
  if (c.edited) parts.push(t.editedByHand);
  if (c.stale) {
    parts.push(
      c.staleProjects.length > 0 ? t.outOfDateIn(c.staleProjects.join(", ")) : t.outOfDate,
    );
  }
  return parts.join(" · ");
}

function weekLabel(key: string): string {
  return key.slice(key.indexOf("W"));
}

function monthLabel(key: string, first: boolean, lang: string): string {
  const [y, m] = key.split("-").map(Number);
  const name = new Date(Date.UTC(y, m - 1, 1)).toLocaleString(lang, {
    month: "short",
    timeZone: "UTC",
  });
  return first || m === 1 ? `${name} ${y}` : name;
}

/** Final, draft and missing reports of one kind as a thin proportional bar. */
function ReportFigure({ label, totals }: { label: string; totals: LantaiKindTotals }) {
  const t = useT().lantai;
  const drafts = totals.reports - totals.finals;
  const title = t.reportFigure(
    totals.finals,
    drafts,
    totals.periods - totals.reports,
    totals.edited,
  );
  return (
    <div className="stats-token-figure" title={title}>
      <div className="stats-kpi-label">{label}</div>
      <div className="lt-figure-value">
        <span className="stats-token-value">{totals.reports}</span>
        <span className="stats-kpi-denom">/ {totals.periods}</span>
        {totals.stale > 0 && (
          <span className="lt-stale-count" title={t.staleCount(totals.stale)}>
            {totals.stale}
          </span>
        )}
      </div>
      <div className="lt-meter">
        <span className="lt-cell-final" style={{ width: `${pct(totals.finals, totals.periods)}%` }} />
        <span className="lt-cell-draft" style={{ width: `${pct(drafts, totals.periods)}%` }} />
      </div>
    </div>
  );
}

function DigestMeter({ tally }: { tally: LantaiDigestTally }) {
  const n = active(tally);
  return (
    <div className="lt-meter">
      {DIGEST_PARTS.map((p) => (
        <span key={p} className={`lt-${p}`} style={{ width: `${pct(tally[p], n)}%` }} />
      ))}
    </div>
  );
}

export function StatsLantai({
  data,
  loading,
  error,
  onJumpToDay,
  onJumpToWeek,
  onJumpToMonth,
}: StatsLantaiProps) {
  const { lang, t: tr } = useLanguage();
  const t = tr.lantai;
  const head = (
    <div className="stats-card-head">
      <div className="stats-card-title">
        <span>{t.title}</span>
      </div>
      {loading && <span className="muted stats-card-meta">{t.checking}</span>}
    </div>
  );

  if (!data) {
    return (
      <section className="stats-card lt-card">
        {head}
        <div className="muted stats-token-empty">{error ? t.couldNotLoad(error) : "…"}</div>
      </section>
    );
  }

  const [fy, fm, fd] = data.from.split("-").map(Number);
  const from = Date.UTC(fy, fm - 1, fd);
  const cols = dayIndex(data.to, from) + 1;
  const last = cols;
  // Column span of a period, clipped to the window; null when it lies outside.
  const span = (c: LantaiPeriodCell) => {
    const a = Math.max(1, dayIndex(c.firstDay, from) + 1);
    const b = Math.min(last, dayIndex(c.lastDay, from) + 1);
    return a > b ? null : `${a} / ${b + 1}`;
  };
  const digestMax = Math.max(1, ...data.digestDays.map((d) => active(d.tally)));
  const digestActive = active(data.digest);

  const periodRow = (
    track: "months" | "weeks" | "days",
    cells: LantaiPeriodCell[],
    text: (c: LantaiPeriodCell, i: number) => string,
    onOpen: (key: string) => void,
  ) => (
    <>
      <div className="lt-row-label">{t[track]}</div>
      <div className={`lt-track lt-period-track lt-track-${track}`}>
        {cells.map((c, i) => {
          const column = span(c);
          if (!column) return null;
          return (
            <button
              key={c.key}
              type="button"
              className={`lt-cell lt-cell-${c.status}${c.stale ? " stale" : ""}`}
              style={{ gridColumn: column }}
              title={cellTitle(c, t)}
              aria-label={cellTitle(c, t)}
              onClick={() => onOpen(c.key)}
            >
              <span className="lt-cell-text">{text(c, i)}</span>
            </button>
          );
        })}
      </div>
    </>
  );

  return (
    <section className="stats-card lt-card">
      {head}

      <div className="stats-token-figures">
        <ReportFigure label={t.daily} totals={data.daily} />
        <ReportFigure label={t.weekly} totals={data.weekly} />
        <ReportFigure label={t.monthly} totals={data.monthly} />
        <div
          className="stats-token-figure"
          title={[
            tallyTitle(data.digest, t),
            t.split(formatInt(data.splitSlices)),
            t.incremental(formatInt(data.incrementalUpdates)),
            t.briefs(formatInt(data.projectBriefs)),
            data.engine,
          ]
            .filter(Boolean)
            .join(" · ")}
        >
          <div className="stats-kpi-label">{t.digestCache}</div>
          <div className="lt-figure-value">
            <span className="stats-token-value">
              {digestActive > 0 ? formatPct(pct(data.digest.fresh, digestActive)) : "–"}
            </span>
            <span className="stats-kpi-denom">
              {formatInt(data.digest.fresh)} / {formatInt(digestActive)}
            </span>
          </div>
          <DigestMeter tally={data.digest} />
        </div>
      </div>

      <div
        className={`lt-timeline${cols > 60 ? " dense" : ""}${cols > 150 ? " crowded" : ""}`}
        style={{ ["--lt-cols" as string]: cols }}
      >
        {periodRow("months", data.months, (c, i) => monthLabel(c.key, i === 0, lang), onJumpToMonth)}
        {periodRow("weeks", data.weeks, (c) => weekLabel(c.key), onJumpToWeek)}
        {periodRow("days", data.days, () => "", onJumpToDay)}

        <div className="lt-row-label lt-digest-label">{t.digests}</div>
        <div className="lt-track lt-digest-track">
          {data.digestDays.map((d) => {
            const n = active(d.tally);
            if (n === 0) return null;
            const title = `${d.date} · ${tallyTitle(d.tally, t)}`;
            return (
              <button
                key={d.date}
                type="button"
                className="lt-digest-bar"
                style={{
                  gridColumn: dayIndex(d.date, from) + 1,
                  height: `${Math.max(6, (n / digestMax) * 100)}%`,
                }}
                title={title}
                aria-label={title}
                onClick={() => onJumpToDay(d.date)}
              >
                {DIGEST_PARTS.map((p) =>
                  d.tally[p] > 0 ? (
                    <span
                      key={p}
                      className={`lt-${p}`}
                      style={{ height: `${pct(d.tally[p], n)}%` }}
                    />
                  ) : null,
                )}
              </button>
            );
          })}
        </div>
      </div>

      <div className="lt-legend">
        <div className="stats-legend">
          {(["final", "draft", "none", "ignored"] as const).map((s) => (
            <span key={s} className="stats-legend-item">
              <span className={`lt-swatch lt-cell-${s}`} />
              <span>{t.status[s]}</span>
            </span>
          ))}
          <span className="stats-legend-item">
            <span className="lt-swatch lt-swatch-stale" />
            <span>{t.legendOutOfDate}</span>
          </span>
        </div>
        <div className="stats-legend">
          {DIGEST_PARTS.map((p) => (
            <span key={p} className="stats-legend-item">
              <span className={`lt-swatch lt-${p}`} />
              <span>{t.digest[p]}</span>
            </span>
          ))}
        </div>
      </div>

      {data.agents.length > 0 && (
        <div className="lt-agents">
          {data.agents.map((a) => {
            const n = active(a.tally);
            return (
              <div key={a.agent} className="lt-agent" title={tallyTitle(a.tally, t)}>
                <span className="stats-agent-name">{a.name}</span>
                <span className="lt-agent-pct">{n > 0 ? formatPct(pct(a.tally.fresh, n)) : "–"}</span>
                <DigestMeter tally={a.tally} />
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
