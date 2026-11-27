import { useEffect, useState } from "react";
import { api } from "../../api";
import { useT } from "../../i18n";
import type { AppIcon } from "../../types";
import { errorText } from "../../errors";

/** Settings > Appearance: pick the Dock / Finder icon from the bundled catalogue. */

// The same PNGs the backend compiles in; keyed by file name (= icon id).
export const APP_ICON_IMAGES: Record<string, string> = Object.fromEntries(
  Object.entries(
    import.meta.glob("../../../src-tauri/app-icons/*.png", {
      eager: true,
      query: "?url",
      import: "default",
    }) as Record<string, string>,
  ).map(([path, url]) => [path.split("/").pop()!.replace(/\.png$/, ""), url]),
);

export function AppIconPicker() {
  const t = useT();
  const [icons, setIcons] = useState<AppIcon[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .listAppIcons()
      .then(setIcons)
      .catch((e) => setError(errorText(e)));
  }, []);

  const choose = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await api.setAppIcon(id);
      setIcons((list) => list.map((i) => ({ ...i, current: i.id === id })));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-card">
      <div className="settings-card-head">
        <h3>{t.appIcon.title}</h3>
      </div>
      {error && <div className="warning-list">{error}</div>}
      <div className="app-icon-grid">
        {icons.map((icon) => (
          <button
            key={icon.id}
            type="button"
            className={`app-icon-option ${icon.current ? "active" : ""}`}
            disabled={busy}
            title={icon.label}
            onClick={() => void choose(icon.id)}
          >
            <img src={APP_ICON_IMAGES[icon.id]} alt={icon.label} draggable={false} />
            <span>{icon.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
