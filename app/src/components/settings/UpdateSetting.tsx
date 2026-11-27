// Settings card for update checks: the automatic check (off by default) and a manual check.
// The result shows here and in the main area's banner; nothing is ever downloaded.
import type { UpdateCheckApi } from "../../hooks/useUpdateCheck";
import { useT } from "../../i18n";
import { InfoTip } from "../InfoTip";
import { UpdateBanner } from "../UpdateBanner";

export function UpdateSetting({ updates }: { updates: UpdateCheckApi }) {
  const t = useT().updates;
  const { settings, checking, banner } = updates;
  return (
    <div className="settings-card">
      <div className="settings-row clean">
        <div>
          <div className="settings-row-title title-with-tip">
            {t.title}
            <InfoTip>{t.tip}</InfoTip>
          </div>
          {settings && <div className="muted-line">{t.current(settings.currentVersion)}</div>}
        </div>
        <div className="settings-inline-actions">
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={settings?.checkUpdates ?? false}
              disabled={!settings}
              aria-label={t.title}
              onChange={(e) => void updates.setAutoCheck(e.target.checked)}
            />
          </label>
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            disabled={checking}
            onClick={() => void updates.checkNow(true)}
          >
            {checking ? t.checking : t.checkNow}
          </button>
        </div>
      </div>
      {banner && (
        <UpdateBanner
          banner={banner}
          onDismiss={updates.dismissBanner}
          onSkip={updates.skipVersion}
        />
      )}
    </div>
  );
}
