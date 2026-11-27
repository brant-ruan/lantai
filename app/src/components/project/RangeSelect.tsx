// The time range of the project view, the same control on every project tab.
import { useT } from "../../i18n";
import type { ProjectRangeKey } from "../../types";

export function RangeSelect({
  value,
  onChange,
}: {
  value: ProjectRangeKey;
  onChange: (range: ProjectRangeKey) => void;
}) {
  const t = useT().rollup;
  return (
    <div className="ctl-group">
      <select
        className="ctl-select"
        value={value}
        onChange={(e) => onChange(e.target.value as ProjectRangeKey)}
        aria-label={t.timeRange}
      >
        <option value="7d">{t.last7}</option>
        <option value="14d">{t.last14}</option>
        <option value="30d">{t.last30}</option>
        <option value="all">{t.allTime}</option>
      </select>
    </div>
  );
}
