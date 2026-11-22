import { useT } from "../i18n";
import type { AgentSource, CompatReason } from "../types";

/**
 * Settings > Agents: every coding agent Lantai knows, where it keeps its
 * sessions, whether those directories exist here, and whether to scan it.
 * Agents without an adapter yet are listed but cannot be enabled. Where the
 * adapter checks versions, the badge says whether the local data's format is
 * one it reads, so a detected directory is never taken as full support.
 * Turning an agent off only stops scanning it: its stored sessions stay visible
 * (projects, notes and report anchors point at them), and the row says they are
 * no longer updated.
 */
export function AgentsView({
  sources,
  onUpdate,
}: {
  sources: AgentSource[];
  onUpdate: (agent: string, enabled: boolean, preDigest: boolean) => void;
}) {
  const t = useT().agents;
  return (
    <div className="settings-card">
      {sources.map((s, i) => (
        <div
          key={s.agent}
          className={`settings-row agent-row ${i === 0 ? "clean" : ""}`}
        >
          <div className="agent-row-main">
            <div className="settings-row-title agent-row-title">
              {s.name}
              {!s.supported ? (
                <span className="badge">{t.comingSoon}</span>
              ) : !s.detected ? (
                <span className="badge">{t.notFound}</span>
              ) : s.compatibility === "unsupported" ? (
                <span className="badge badge-warn">{t.unsupported}</span>
              ) : s.compatibility === "partial" ? (
                <span className="badge badge-warn">{t.partial}</span>
              ) : (
                <span className="badge badge-ok">{t.detected}</span>
              )}
            </div>
            {s.roots.map((r) => (
              <div
                key={r.path}
                className={`agent-root ${r.exists ? "" : "missing"}`}
                title={r.exists ? t.dirExists : t.dirMissing}
              >
                {r.path}
              </div>
            ))}
            {s.versions.length > 0 && (
              <div className="agent-versions">
                {t.versions}: {s.versions.join(", ")}
              </div>
            )}
            {s.storedSessions > 0 &&
              (s.supported && !s.enabled ? (
                <div className="agent-stored frozen">
                  {t.storedFrozen(s.storedSessions)}
                </div>
              ) : (
                <div className="agent-stored">{t.stored(s.storedSessions)}</div>
              ))}
            {s.compatReason && (
              <div className="agent-compat-note">
                {reasonText(t, s.compatReason)}
              </div>
            )}
          </div>
          <label
            className={`toggle-row ${s.supported ? "" : "disabled"}`}
            title={s.supported ? undefined : t.noAdapter}
          >
            <input
              type="checkbox"
              checked={s.enabled}
              disabled={!s.supported}
              onChange={(e) =>
                onUpdate(
                  s.agent,
                  e.target.checked,
                  e.target.checked && s.preDigest,
                )
              }
            />
            <span>{t.enabled}</span>
          </label>
        </div>
      ))}
    </div>
  );
}

function reasonText(
  t: ReturnType<typeof useT>["agents"],
  r: CompatReason,
): string {
  switch (r.kind) {
    case "newerFormat":
      return t.newerFormat;
    case "sessionsInNewerFormat":
      return t.sessionsInNewerFormat(r.count);
    case "untestedVersions":
      return t.untestedVersions(r.versions.join(", "));
    case "unrecognizedLayout":
      return t.unrecognizedLayout;
    case "probeFailed":
      return t.probeFailed(r.message);
  }
}
