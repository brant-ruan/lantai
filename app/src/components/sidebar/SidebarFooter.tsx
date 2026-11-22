// Sidebar footer: scan status, the Scan button, and the Search, Stats and Settings icon buttons.

import { useT } from "../../i18n";

const ICON_PROPS = {
  width: 15,
  height: 15,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

export function SidebarFooter({
  scanning,
  scanStatus,
  onScan,
  statsActive,
  onToggleStats,
  searchActive,
  onToggleSearch,
  onOpenSettings,
}: {
  scanning: boolean;
  scanStatus: string | null;
  onScan: () => void;
  statsActive: boolean;
  onToggleStats: () => void;
  searchActive: boolean;
  onToggleSearch: () => void;
  onOpenSettings: () => void;
}) {
  const t = useT();
  return (
    <div className="sidebar-footer">
      {scanStatus && <div className="sidebar-note">{scanStatus}</div>}
      <div className="footer-actions">
        <button
          type="button"
          className="btn btn-secondary btn-sm flex-1"
          disabled={scanning}
          onClick={onScan}
        >
          {scanning ? t.sidebar.scanning : t.sidebar.scan}
        </button>
        <button
          type="button"
          className={`btn btn-secondary btn-sm btn-icon-sq ${searchActive ? "active" : ""}`}
          onClick={onToggleSearch}
          title={t.sidebar.search}
          aria-label={t.sidebar.search}
          aria-pressed={searchActive}
        >
          <svg {...ICON_PROPS}>
            <circle cx="11" cy="11" r="7" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
        </button>
        <button
          type="button"
          className={`btn btn-secondary btn-sm btn-icon-sq ${statsActive ? "active" : ""}`}
          onClick={onToggleStats}
          title={t.sidebar.stats}
          aria-label={t.sidebar.stats}
          aria-pressed={statsActive}
        >
          <svg {...ICON_PROPS}>
            <line x1="18" y1="20" x2="18" y2="10" />
            <line x1="12" y1="20" x2="12" y2="4" />
            <line x1="6" y1="20" x2="6" y2="14" />
          </svg>
        </button>
        <button
          type="button"
          className="btn btn-secondary btn-sm btn-icon-sq"
          onClick={onOpenSettings}
          title={t.sidebar.settings}
          aria-label={t.sidebar.settings}
        >
          <svg {...ICON_PROPS}>
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </button>
      </div>
    </div>
  );
}
