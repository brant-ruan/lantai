// Search view: a query box with filters, report sections that match, and matching session
// turns grouped by day. Clicking a report opens its period view; clicking a turn opens the
// slice drawer at that turn. Escape leaves the view (after the drawer or a modal, which own
// Escape while open).

import { useEffect, useMemo, useRef } from "react";
import { AgentBadge } from "./AgentBadge";
import { modalOpen } from "../dom";
import { useLanguage, useT, weekdayName } from "../i18n";
import type { SearchView as SearchState } from "../hooks/useSearchView";
import type {
  FieldSnippet,
  ReportRef,
  SearchLayer,
  SliceSearchHit,
  SnippetPart,
} from "../types";

const LAYERS: SearchLayer[] = ["all", "reports", "sessions"];

interface SearchViewProps {
  search: SearchState;
  projects: { id: number; name: string }[];
  agents: { agent: string; name: string }[];
  onOpenReport: (kind: string, periodKey: string) => void;
  onOpenSlice: (sliceId: number, turnIndex?: number) => void;
  /** Whether the slice drawer is open; it closes on Escape first. */
  drawerOpen: boolean;
  /** Return to the view shown before Search. */
  onExit: () => void;
}

function Snippet({ parts }: { parts: SnippetPart[] }) {
  return (
    <span className="search-snippet">
      {parts.map((p, i) =>
        p.hit ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>,
      )}
    </span>
  );
}

export function SearchView({
  search,
  projects,
  agents,
  onOpenReport,
  onOpenSlice,
  drawerOpen,
  onExit,
}: SearchViewProps) {
  const t = useT();
  const s = t.search;
  const { lang } = useLanguage();
  const { params, update, results, loading, error } = search;
  const inputRef = useRef<HTMLInputElement>(null);

  // Escape returns to the previous view, from anywhere in it (the search box included). The
  // query is kept for next time.
  useEffect(() => {
    if (drawerOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || modalOpen()) return;
      e.preventDefault();
      onExit();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drawerOpen, onExit]);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [search.focusSeq]);

  const byDay = useMemo(() => {
    const days: { date: string; slices: SliceSearchHit[] }[] = [];
    for (const hit of results?.sessions ?? []) {
      const last = days[days.length - 1];
      if (last && last.date === hit.localDate) last.slices.push(hit);
      else days.push({ date: hit.localDate, slices: [hit] });
    }
    return days;
  }, [results]);

  const turnCount = (results?.sessions ?? []).reduce(
    (n, h) => n + h.turns.length,
    0,
  );
  const showReports = params.layer !== "sessions";
  const showSessions = params.layer !== "reports";
  const hasQuery = params.query.trim().length > 0;
  const nothing =
    results &&
    (!showReports || results.reports.length === 0) &&
    (!showSessions || results.sessions.length === 0);

  const reportLabel = (r: ReportRef) =>
    `${t.reportKinds[r.kind] ?? r.kind} ${r.periodKey}`;

  const citedBy = (refs: ReportRef[]) =>
    refs.length > 0 && (
      <span className="search-cited">
        <span className="muted">{s.citedBy}</span>
        {refs.map((r) => (
          <button
            key={`${r.kind}|${r.periodKey}`}
            type="button"
            className="search-chip"
            title={s.openReport}
            onClick={(e) => {
              e.stopPropagation();
              onOpenReport(r.kind, r.periodKey);
            }}
          >
            {reportLabel(r)}
          </button>
        ))}
      </span>
    );

  const fieldRows = (snippets: FieldSnippet[]) =>
    snippets.map((sn, i) => (
      <div key={i} className="search-field-row">
        <span className="search-field">{s.fields[sn.field] ?? sn.field}</span>
        <Snippet parts={sn.parts} />
      </div>
    ));

  return (
    <section className="panel search-panel">
      <div className="panel-header">
        <div className="topbar-left search-box">
          <input
            ref={inputRef}
            type="search"
            value={params.query}
            placeholder={s.placeholder}
            title={s.hint}
            spellCheck={false}
            onChange={(e) => update({ query: e.target.value })}
            onKeyDown={(e) => {
              // A search field clears itself on Escape; keep the query, leave the view instead.
              if (e.key === "Escape") e.preventDefault();
              if (e.key === "Enter") void search.refresh();
            }}
          />
          {loading && <span className="muted">{s.searching}</span>}
          {!loading && results && hasQuery && (
            <span className="muted search-elapsed">
              {s.elapsed(results.elapsedMs)}
            </span>
          )}
        </div>
        <div className="topbar-right">
          <div className="pill-group" role="tablist" aria-label={s.layerLabel}>
            {LAYERS.map((l) => (
              <button
                key={l}
                type="button"
                role="tab"
                aria-selected={params.layer === l}
                className={`pill ${params.layer === l ? "active" : ""}`}
                onClick={() => update({ layer: l })}
              >
                {s.layers[l]}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="search-filters">
        <select
          value={params.projectId ?? ""}
          onChange={(e) =>
            update({
              projectId: e.target.value ? Number(e.target.value) : null,
            })
          }
        >
          <option value="">{s.allProjects}</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <select
          value={params.agent ?? ""}
          title={s.agentReportsNote}
          onChange={(e) => update({ agent: e.target.value || null })}
        >
          <option value="">{s.allAgents}</option>
          {agents.map((a) => (
            <option key={a.agent} value={a.agent}>
              {a.name}
            </option>
          ))}
        </select>
        <label className="search-date">
          <span>{s.since}</span>
          <input
            type="date"
            value={params.since ?? ""}
            onChange={(e) => update({ since: e.target.value || null })}
          />
        </label>
        <label className="search-date">
          <span>{s.until}</span>
          <input
            type="date"
            value={params.until ?? ""}
            onChange={(e) => update({ until: e.target.value || null })}
          />
        </label>
        <label className="stats-toggle" title={s.includeIgnoredTitle}>
          <input
            type="checkbox"
            checked={params.includeIgnored}
            onChange={(e) => update({ includeIgnored: e.target.checked })}
          />
          <span>{s.includeIgnored}</span>
        </label>
        <label className="stats-toggle" title={s.toolOutputTitle}>
          <input
            type="checkbox"
            checked={params.includeToolOutput}
            onChange={(e) => update({ includeToolOutput: e.target.checked })}
          />
          <span>{s.toolOutput}</span>
        </label>
      </div>

      <div className="search-scroll">
        {error && <div className="banner banner-err">{error}</div>}
        {!hasQuery && <div className="empty-state">{s.empty}</div>}
        {hasQuery && nothing && !loading && (
          <div className="empty-state">{s.noResults}</div>
        )}

        {hasQuery && results && showReports && results.reports.length > 0 && (
          <div className="search-group">
            <h3 className="search-group-title">
              {s.reports(results.reports.length)}
            </h3>
            {results.reports.map((hit, i) => (
              <button
                key={`${hit.kind}|${hit.periodKey}|${hit.section ?? ""}|${i}`}
                type="button"
                className="search-hit"
                title={s.openReport}
                onClick={() => onOpenReport(hit.kind, hit.periodKey)}
              >
                <div className="search-hit-head">
                  <span className="badge">
                    {t.reportKinds[hit.kind] ?? hit.kind}
                  </span>
                  <span className="search-period">
                    {hit.periodKey}
                    {hit.kind === "daily" && (
                      <span className="muted">
                        {" "}
                        {weekdayName(lang, hit.periodKey)}
                      </span>
                    )}
                  </span>
                  <span className="search-section">
                    {hit.section ?? (hit.human ? t.human.dayNotes : s.header)}
                  </span>
                  {hit.human && <span className="badge badge-human">{s.note}</span>}
                </div>
                {hit.snippets.map((sn, j) => (
                  <div key={j} className="search-field-row">
                    <Snippet parts={sn} />
                  </div>
                ))}
              </button>
            ))}
            {results.reportsTruncated && (
              <div className="muted search-note">{s.reportsTruncated}</div>
            )}
          </div>
        )}

        {hasQuery && results && showSessions && results.sessions.length > 0 && (
          <div className="search-group">
            <h3 className="search-group-title">
              {s.sessions(results.sessions.length, turnCount)}
            </h3>
            {byDay.map((day) => (
              <div key={day.date} className="search-day">
                <div className="search-day-title">
                  {day.date}{" "}
                  <span className="muted">{weekdayName(lang, day.date)}</span>
                </div>
                {day.slices.map((hit) => (
                  <div key={hit.sliceId} className="search-slice">
                    <button
                      type="button"
                      className="search-slice-head"
                      onClick={() => onOpenSlice(hit.sliceId)}
                    >
                      <AgentBadge agent={hit.agent} name={hit.agentName} />
                      <span className="search-slice-title">
                        {hit.title || s.untitled}
                      </span>
                      <span className="muted">
                        {hit.projectName ?? s.unassigned}
                      </span>
                      {hit.ignored && (
                        <span className="badge">{s.ignored}</span>
                      )}
                      <span className="muted search-slice-id">
                        S{hit.sliceId}
                      </span>
                    </button>
                    {hit.titleSnippets.length > 0 && (
                      <div className="search-turn search-turn-static">
                        {fieldRows(hit.titleSnippets)}
                      </div>
                    )}
                    {citedBy(hit.citedBy)}
                    {hit.turns.map((turn) => (
                      <div
                        key={turn.turnIndex}
                        role="button"
                        tabIndex={0}
                        className="search-turn"
                        title={s.openTurn}
                        onClick={() => onOpenSlice(hit.sliceId, turn.turnIndex)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter")
                            onOpenSlice(hit.sliceId, turn.turnIndex);
                        }}
                      >
                        <span className="search-turn-index">
                          T{turn.turnIndex}
                        </span>
                        <div className="search-turn-body">
                          {fieldRows(turn.snippets)}
                          {citedBy(
                            turn.citedBy.filter(
                              (r) =>
                                !hit.citedBy.some(
                                  (c) =>
                                    c.kind === r.kind &&
                                    c.periodKey === r.periodKey,
                                ),
                            ),
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            ))}
            {results.sessionsTruncated && (
              <div className="muted search-note">{s.sessionsTruncated}</div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
