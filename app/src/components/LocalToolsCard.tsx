import { useState } from "react";
import { api } from "../api";
import { InfoTip } from "./InfoTip";
import { useT } from "../i18n";
import type { LocalTool, ModelSettings } from "../types";

/**
 * Settings > Models: agent CLIs on this Mac. Finding runs only when asked and only runs each
 * tool's version command; tools that pass the bar (`docs/SPEC.md`, "Agent CLIs") can be added,
 * the others are listed with why not.
 */

export function LocalToolsCard({
  busy,
  apply,
}: {
  busy: boolean;
  /** Runs a settings change and reports whether it succeeded. */
  apply: (fn: () => Promise<ModelSettings>) => Promise<boolean>;
}) {
  const tm = useT().models;
  const [tools, setTools] = useState<LocalTool[] | null>(null);
  const [finding, setFinding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const find = async () => {
    setFinding(true);
    setError(null);
    try {
      setTools(await api.findLocalTools());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setFinding(false);
    }
  };

  const add = async (command: string) => {
    if (await apply(() => api.addLocalTool(command))) await find();
  };

  return (
    <div className="settings-card">
      <div className="settings-card-head">
        <h3 className="title-with-tip">
          {tm.localTools}
          <InfoTip>{tm.localToolsTip}</InfoTip>
        </h3>
        <button
          type="button"
          className="btn btn-secondary btn-xs"
          disabled={finding}
          onClick={() => void find()}
        >
          {finding ? tm.finding : tools ? tm.findAgain : tm.findTools}
        </button>
      </div>
      {error && <div className="warning-list">{error}</div>}
      {!tools && <div className="muted-line">{tm.localToolsIntro}</div>}
      {tools && tools.length === 0 && <div className="muted-line">{tm.noToolsFound}</div>}
      {tools && tools.length > 0 && (
        <div className="settings-list">
          {tools.map((t) => (
            <div key={t.command} className="settings-item-row">
              <div className="settings-item-main">
                <div className="slice-title-line">
                  <strong>{t.product}</strong>
                  <span className="badge">{t.version ?? tm.versionUnknown}</span>
                  {t.configuredAs && (
                    <span className="badge badge-ok">{tm.addedAs(t.configuredAs)}</span>
                  )}
                  {!t.engine && <span className="badge badge-warn">{tm.cannotGenerate}</span>}
                </div>
                <div className="muted-line local-tool-path">
                  {t.path}
                  {!t.engine && ` · ${t.gaps.map((g) => tm.gaps[g] ?? g).join("; ")}`}
                </div>
              </div>
              {t.engine && !t.configuredAs && (
                <div className="settings-item-actions">
                  <button
                    type="button"
                    className="btn btn-primary btn-xs"
                    disabled={busy}
                    onClick={() => void add(t.command)}
                  >
                    {tm.addTool}
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
