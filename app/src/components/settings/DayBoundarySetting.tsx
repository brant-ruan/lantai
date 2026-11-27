// Settings row for the day boundary: the local hour a new day starts. Changing it re-cuts
// every stored session into days, then asks the app to refresh its views.
import { useEffect, useState } from "react";
import { api } from "../../api";
import { useT } from "../../i18n";
import type { DayBoundary } from "../../types";
import { InfoTip } from "../InfoTip";
import { errorText } from "../../errors";

const hourLabel = (h: number) => `${String(h).padStart(2, "0")}:00`;

export function DayBoundarySetting({ onChanged }: { onChanged: () => Promise<void> }) {
  const t = useT().dayBoundary;
  const [boundary, setBoundary] = useState<DayBoundary | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.getDayBoundary().then(setBoundary, (err) => setError(errorText(err)));
  }, []);

  const change = async (hour: number) => {
    setBusy(true);
    setError(null);
    try {
      setBoundary(await api.setDayBoundary(hour));
      await onChanged();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-card">
      <div className="settings-row clean">
        <div>
          <div className="settings-row-title title-with-tip">
            {t.title}
            <InfoTip>{t.tip}</InfoTip>
          </div>
          {busy && <div className="muted-line">{t.refiling}</div>}
          {error && <div className="muted-line" style={{ color: "var(--err)" }}>{error}</div>}
        </div>
        <select
          value={boundary?.hour ?? ""}
          disabled={!boundary || busy}
          onChange={(e) => void change(Number(e.target.value))}
        >
          {boundary &&
            Array.from({ length: boundary.maxHour + 1 }, (_, h) => (
              <option key={h} value={h}>
                {hourLabel(h)}
                {h === 0 ? t.midnight : ""}
              </option>
            ))}
        </select>
      </div>
    </div>
  );
}
