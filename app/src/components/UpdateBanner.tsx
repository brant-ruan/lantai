// Banner at the top of the main area that reports an update check: a new version (with a
// warning when it brings breaking changes), "up to date" after a manual check, or an error.
import { api } from "../api";
import type { UpdateBanner as Banner } from "../hooks/useUpdateCheck";
import { useT } from "../i18n";

export function UpdateBanner({
  banner,
  onDismiss,
  onSkip,
}: {
  banner: Banner;
  onDismiss: () => void;
  onSkip: () => void;
}) {
  const t = useT();
  const u = t.updates;
  const dismiss = (
    <button type="button" className="btn btn-ghost btn-xs" onClick={onDismiss}>
      {t.workspace.dismiss}
    </button>
  );

  if (banner.kind === "upToDate") {
    return (
      <div className="banner banner-info">
        <span>{u.upToDate(banner.version)}</span>
        {dismiss}
      </div>
    );
  }
  if (banner.kind === "error") {
    return (
      <div className="banner banner-err">
        <span>{u.failed(banner.message)}</span>
        {dismiss}
      </div>
    );
  }
  const { check } = banner;
  return (
    <div className={`banner ${check.breaking ? "banner-warn" : "banner-info"}`}>
      <span>
        {u.available(check.latest)}
        {check.breaking && <> {u.breaking}</>}
      </span>
      <div className="banner-actions">
        <button
          type="button"
          className="btn btn-secondary btn-xs"
          onClick={() => void api.openReleasePage(check.url).catch(() => {})}
        >
          {u.viewRelease}
        </button>
        <button type="button" className="btn btn-ghost btn-xs" onClick={onSkip}>
          {u.skipVersion}
        </button>
        {dismiss}
      </div>
    </div>
  );
}
