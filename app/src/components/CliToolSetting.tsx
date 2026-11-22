// Settings card for the bundled `aj` command: shows whether ~/.local/bin/aj links to this
// app, installs or removes that link, and explains how to put ~/.local/bin on the PATH.
import { useEffect, useState } from "react";
import { api } from "../api";
import { useT } from "../i18n";
import type { CliToolStatus } from "../types";
import { InfoTip } from "./InfoTip";

export function CliToolSetting() {
  const t = useT().cliTool;
  const [status, setStatus] = useState<CliToolStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .cliToolStatus()
      .then(setStatus)
      .catch((err) => setError(String(err)));
  }, []);

  const run = async (task: () => Promise<CliToolStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await task());
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const describe = (s: CliToolStatus): string => {
    if (!s.bundled) return t.notBundled;
    if (s.translocated) return t.translocated;
    switch (s.state) {
      case "installed":
        return t.installed(s.linkPath);
      case "missing":
        return t.missing(s.linkPath);
      case "staleLink":
        return t.stale(s.linkPath);
      case "other":
        return t.other(s.linkPath);
    }
  };

  const canInstall =
    status !== null && status.bundled && !status.translocated && status.state !== "other";
  const linked = status?.state === "installed" || status?.state === "staleLink";

  return (
    <div className="settings-card">
      <div className="settings-row clean">
        <div>
          <div className="settings-row-title title-with-tip">
            {t.title}
            <InfoTip>{t.tip}</InfoTip>
          </div>
          {status && <div className="muted-line backup-message">{describe(status)}</div>}
        </div>
        <div className="settings-inline-actions">
          {linked && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              disabled={busy}
              onClick={() => void run(api.uninstallCliTool)}
            >
              {t.uninstall}
            </button>
          )}
          {canInstall && (
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              disabled={busy}
              onClick={() => void run(api.installCliTool)}
            >
              {status?.state === "missing" ? t.install : t.reinstall}
            </button>
          )}
        </div>
      </div>
      {status?.state === "installed" && !status.onPath && (
        <div className="cli-path-hint">
          <div className="muted-line">{t.notOnPath}</div>
          <code className="cli-path-line">{t.pathLine}</code>
        </div>
      )}
      {error && (
        <div className="muted-line backup-message" style={{ color: "var(--err)" }}>
          {error}
        </div>
      )}
    </div>
  );
}
