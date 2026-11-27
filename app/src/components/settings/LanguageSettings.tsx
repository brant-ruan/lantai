// Settings > Language: the interface language and the report language, chosen apart.

import { useState } from "react";
import { UI_LANGUAGES, resolveUiLanguage, systemLanguages, useLanguage } from "../../i18n";
import { InfoTip } from "../InfoTip";
import { errorText } from "../../errors";

export function LanguageSettings() {
  const {
    t: tr,
    uiSetting,
    setUiSetting,
    reportLanguage,
    reportLanguages,
    setReportLanguage,
  } = useLanguage();
  const t = tr.languageSettings;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const systemTag = resolveUiLanguage("auto", systemLanguages());
  const systemName = UI_LANGUAGES.find((l) => l.tag === systemTag)?.nativeName ?? systemTag;

  const run = async (action: () => Promise<void>, done?: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await action();
      if (done) setNotice(done);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-stack">
      <div className="settings-card">
        <div className="settings-row clean">
          <div>
            <div className="settings-row-title title-with-tip">
              {t.interface}
              <InfoTip>{t.interfaceTip}</InfoTip>
            </div>
          </div>
          <select
            value={uiSetting}
            disabled={busy}
            onChange={(e) => void run(() => setUiSetting(e.target.value))}
          >
            <option value="auto">{t.auto(systemName)}</option>
            {UI_LANGUAGES.map((l) => (
              <option key={l.tag} value={l.tag}>
                {l.nativeName}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-row clean">
          <div>
            <div className="settings-row-title title-with-tip">
              {t.report}
              <InfoTip>{t.reportTip}</InfoTip>
            </div>
            {notice && <div className="muted-line">{notice}</div>}
            {error && (
              <div className="muted-line" style={{ color: "var(--err)" }}>
                {error}
              </div>
            )}
          </div>
          <select
            value={reportLanguage}
            disabled={busy || reportLanguages.length === 0}
            onChange={(e) => {
              const tag = e.target.value;
              const name = reportLanguages.find((l) => l.tag === tag)?.nativeName ?? tag;
              void run(() => setReportLanguage(tag), t.changed(name));
            }}
          >
            {reportLanguages.map((l) => (
              <option key={l.tag} value={l.tag} title={l.tuned ? t.tuned : t.borrowed}>
                {l.nativeName}
              </option>
            ))}
          </select>
        </div>
      </div>
    </div>
  );
}
