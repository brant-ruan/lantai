// Settings card for backups: save the database and the settings to a zip file, and restore
// them from one after a confirmation. A restore replaces everything, so the window reloads.
import { useState } from "react";
import { api } from "../../api";
import { formatDateTime, useLanguage } from "../../i18n";
import type { BackupInfo } from "../../types";
import { InfoTip } from "../InfoTip";
import { errorText } from "../../errors";

/** How long the result of a restore stays on screen before the window reloads. */
const RELOAD_DELAY_MS = 1500;

export function BackupSetting() {
  const { lang, t: tr } = useLanguage();
  const t = tr.backup;
  const [busy, setBusy] = useState<"export" | "restore" | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A backup picked for restoring, waiting for confirmation.
  const [pending, setPending] = useState<BackupInfo | null>(null);

  const run = async (kind: "export" | "restore", task: () => Promise<void>) => {
    setBusy(kind);
    setError(null);
    setMessage(null);
    try {
      await task();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(null);
    }
  };

  const exportBackup = () =>
    run("export", async () => {
      const saved = await api.exportBackup(t.exportPrompt);
      if (saved) setMessage(t.exported(saved.path));
    });

  const pickBackup = () =>
    run("restore", async () => {
      setPending(await api.pickBackup(t.restorePrompt));
    });

  const restore = (backup: BackupInfo) =>
    run("restore", async () => {
      const restored = await api.restoreBackup(backup.path);
      setPending(null);
      setMessage(t.restored(restored.previous));
      window.setTimeout(() => window.location.reload(), RELOAD_DELAY_MS);
    });

  return (
    <div className="settings-card">
      <div className="settings-row clean">
        <div>
          <div className="settings-row-title title-with-tip">
            {t.exportTitle}
            <InfoTip>{t.exportTip}</InfoTip>
          </div>
          {busy === "export" && <div className="muted-line">{t.exporting}</div>}
        </div>
        <button
          type="button"
          className="btn btn-secondary btn-sm"
          disabled={busy !== null}
          onClick={() => void exportBackup()}
        >
          {t.export}
        </button>
      </div>
      <div className="settings-row">
        <div>
          <div className="settings-row-title title-with-tip">
            {t.restoreTitle}
            <InfoTip>{t.restoreTip}</InfoTip>
          </div>
          {busy === "restore" && pending && <div className="muted-line">{t.restoring}</div>}
        </div>
        <button
          type="button"
          className="btn btn-secondary btn-sm"
          disabled={busy !== null || pending !== null}
          onClick={() => void pickBackup()}
        >
          {t.restore}
        </button>
      </div>
      {pending && (
        <div className="banner banner-warn backup-confirm">
          <span>{t.confirm(formatDateTime(lang, pending.createdAt), pending.appVersion)}</span>
          <div className="banner-actions">
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              disabled={busy !== null}
              onClick={() => setPending(null)}
            >
              {tr.common.cancel}
            </button>
            <button
              type="button"
              className="btn btn-danger btn-xs"
              disabled={busy !== null}
              onClick={() => void restore(pending)}
            >
              {t.confirmButton}
            </button>
          </div>
        </div>
      )}
      {message && <div className="muted-line backup-message">{message}</div>}
      {error && (
        <div className="muted-line backup-message" style={{ color: "var(--err)" }}>
          {error}
        </div>
      )}
    </div>
  );
}
