import { useT } from "../i18n";
import type { Diagnostics } from "../types";

/** Settings > System: local corpus statistics and source adapter health. */

interface HealthViewProps {
  health: Diagnostics | null;
  onRefresh: () => Promise<void>;
}

export function HealthView({
  health,
  onRefresh,
}: HealthViewProps) {
  const t = useT().health;
  if (!health) {
    return <div className="empty-state">{t.loading}</div>;
  }

  return (
    <>
      <div className="settings-card">
        <div className="settings-card-head">
          <h3>{t.title}</h3>
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            onClick={() => void onRefresh()}
          >
            {t.refresh}
          </button>
        </div>

        <div className="stats-grid">
          <div className="stat-card">
            <div className="muted-line">{t.sessions}</div>
            <div className="stat-val">{health.sessions}</div>
          </div>
          <div className="stat-card">
            <div className="muted-line">{t.turns}</div>
            <div className="stat-val">{health.turns}</div>
          </div>
          <div className="stat-card">
            <div className="muted-line">{t.irVersion}</div>
            <div className="stat-val">{health.irVersion}</div>
          </div>
        </div>

        <div className="settings-kv-list">
          <div className="settings-kv-row">
            <span className="meta-k">{t.dataDir}</span>
            <code className="settings-path-code">{health.dataDir}</code>
          </div>
          <div className="settings-kv-row">
            <span className="meta-k">{t.exportDir}</span>
            <code className="settings-path-code">{health.exportDir}</code>
          </div>
        </div>

        <div className="settings-list">
          {health.sources.map((s) => (
            <div key={s.agent} className="settings-item-row">
              <div className="settings-item-main">
                <div className="slice-title-line">
                  <strong>{s.agent}</strong>
                  <span className="muted-line">
                    {t.sourceStats(s.files, s.errorCount, s.unknownCount)}
                  </span>
                </div>
                {s.agentVersions.length > 0 && (
                  <div className="muted-line">
                    {t.agentVersions(
                      s.agentVersions[0],
                      s.agentVersions[s.agentVersions.length - 1],
                      s.agentVersions.length,
                    )}
                  </div>
                )}
                {s.lastError && (
                  <div className="warning-list">{t.latestError(s.lastError)}</div>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
