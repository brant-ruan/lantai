// Checks the database before the interface loads. When it cannot be used as it is, the window
// shows a dialog instead: back up and upgrade data from an older Lantai, or explain why it cannot
// be opened (docs/SPEC.md, "Upgrading the database"). It renders outside `LanguageProvider`,
// which needs the database, so it takes the interface language from the status itself.

import { useEffect, useState, type ReactNode } from "react";
import { api } from "../api";
import { storedTheme } from "../hooks/useAppearance";
import { messagesFor, resolveUiLanguage, systemLanguages } from "../i18n";
import type { DatabaseStatus, UpgradedDatabase } from "../types";
import type { Messages } from "../locales/en";
import { errorText } from "../errors";

type Texts = Messages["database"];

export function DatabaseGate({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<DatabaseStatus | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    // The app applies the theme once it loads; the dialog comes first, so apply it here too.
    document.documentElement.dataset.theme = storedTheme();
    api
      .databaseStatus()
      .then((s) => {
        setStatus(s);
        if (s.database.state === "ready") setReady(true);
      })
      .catch((e) => setLoadError(errorText(e)));
  }, []);

  if (ready) return <>{children}</>;
  if (!status && !loadError) return null;

  const t = messagesFor(resolveUiLanguage(status?.uiLanguage ?? "auto", systemLanguages()))
    .database;
  const quit = () => void api.quitApp().catch(() => {});

  if (!status) {
    return (
      <GateCard
        title={t.errorTitle}
        actions={
          <button type="button" className="btn btn-primary btn-sm" onClick={quit}>
            {t.quit}
          </button>
        }
      >
        <div className="banner banner-err">{loadError}</div>
      </GateCard>
    );
  }

  const db = status.database;
  switch (db.state) {
    case "outdated":
      return (
        <UpgradeDialog
          t={t}
          status={status}
          from={db.from}
          to={db.to}
          onDone={() => setReady(true)}
          onQuit={quit}
        />
      );
    case "tooNew":
      return (
        <GateCard
          title={t.tooNewTitle}
          actions={
            <>
              <button type="button" className="btn btn-ghost btn-sm" onClick={quit}>
                {t.quit}
              </button>
              <button
                type="button"
                className="btn btn-primary btn-sm"
                onClick={() => void api.openReleasePage(status.releaseUrl).catch(() => {})}
              >
                {t.download}
              </button>
            </>
          }
        >
          <p>{t.tooNewLead(db.found, db.known)}</p>
        </GateCard>
      );
    case "tooOld":
      return (
        <GateCard
          title={t.tooOldTitle}
          actions={
            <>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={() => void api.revealInFinder(status.dataDir).catch(() => {})}
              >
                {t.showInFinder}
              </button>
              <button type="button" className="btn btn-primary btn-sm" onClick={quit}>
                {t.quit}
              </button>
            </>
          }
        >
          <p>{t.tooOldLead(db.found)}</p>
          <code className="database-gate-path">{status.dataDir}</code>
        </GateCard>
      );
    case "ready":
      return null;
  }
}

/** Asks before upgrading, runs the backup and the upgrade, then says where the backup is. */
function UpgradeDialog({
  t,
  status,
  from,
  to,
  onDone,
  onQuit,
}: {
  t: Texts;
  status: DatabaseStatus;
  from: number;
  to: number;
  onDone: () => void;
  onQuit: () => void;
}) {
  const [backupDir, setBackupDir] = useState(status.backupDir);
  const [phase, setPhase] = useState<"ask" | "working" | "done" | "failed">("ask");
  const [result, setResult] = useState<UpgradedDatabase | null>(null);
  const [error, setError] = useState<string | null>(null);

  const chooseFolder = async () => {
    const picked = await api.chooseBackupFolder(t.chooseFolderPrompt, backupDir).catch(() => null);
    if (picked) setBackupDir(picked);
  };
  const upgrade = async () => {
    setPhase("working");
    try {
      setResult(
        await api.upgradeDatabase(backupDir === status.backupDir ? null : backupDir),
      );
      setPhase("done");
    } catch (e) {
      setError(errorText(e));
      setPhase("failed");
    }
  };
  const reveal = (path: string) => void api.revealInFinder(path).catch(() => {});

  if (phase === "done" && result) {
    return (
      <GateCard
        title={t.doneTitle}
        actions={
          <>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => reveal(result.backup)}>
              {t.showInFinder}
            </button>
            <button type="button" className="btn btn-primary btn-sm" onClick={onDone}>
              {t.continue}
            </button>
          </>
        }
      >
        <p>{t.backupAt}</p>
        <code className="database-gate-path">{result.backup}</code>
        <p className="muted-line">{t.goBackNote}</p>
      </GateCard>
    );
  }

  if (phase === "failed") {
    return (
      <GateCard
        title={t.failedTitle}
        actions={
          <>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => reveal(backupDir)}>
              {t.showBackups}
            </button>
            <button type="button" className="btn btn-primary btn-sm" onClick={onQuit}>
              {t.quit}
            </button>
          </>
        }
      >
        <div className="banner banner-err">{error}</div>
        <code className="database-gate-path">{backupDir}</code>
        <p className="muted-line">{t.failedNote}</p>
      </GateCard>
    );
  }

  const working = phase === "working";
  return (
    <GateCard
      title={t.upgradeTitle}
      actions={
        <>
          <button type="button" className="btn btn-ghost btn-sm" disabled={working} onClick={onQuit}>
            {t.quit}
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={working}
            onClick={() => void upgrade()}
          >
            {working ? t.upgrading : t.upgrade}
          </button>
        </>
      }
    >
      <p>{t.upgradeLead(from, to)}</p>
      <p>{t.backupTo}</p>
      <div className="database-gate-folder">
        <code className="database-gate-path">{backupDir}</code>
        <button
          type="button"
          className="btn btn-secondary btn-xs"
          disabled={working}
          onClick={() => void chooseFolder()}
        >
          {t.changeFolder}
        </button>
      </div>
      <p className="muted-line">{t.keysNote}</p>
    </GateCard>
  );
}

/** A dialog alone in the window: nothing else is loaded, so it has no backdrop to dismiss. */
function GateCard({
  title,
  children,
  actions,
}: {
  title: string;
  children: ReactNode;
  actions: ReactNode;
}) {
  return (
    <div className="database-gate">
      <div className="confirm-modal" role="alertdialog" aria-modal="true" aria-label={title}>
        <div className="confirm-modal-body">
          <h2>{title}</h2>
          {children}
        </div>
        <div className="export-modal-foot">{actions}</div>
      </div>
    </div>
  );
}
