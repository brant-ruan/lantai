// The source switch shared by the report and project views: the agent and the person together,
// only the agent, or only the person. The choice is kept on this machine and only changes what
// is shown.
import { useState } from "react";
import { useT } from "../../i18n";
import { readItem, storeItem } from "../../storage";
import type { SourceFilter } from "../../types";
import { AgentIcon, HumanIcon, TogetherIcon } from "../icons";

const KEY = "lantai.report.source";
const FILTERS: SourceFilter[] = ["both", "agent", "human"];
const ICONS = { both: TogetherIcon, agent: AgentIcon, human: HumanIcon };

export function useSourceFilter(): [SourceFilter, (f: SourceFilter) => void] {
  const [filter, setFilter] = useState<SourceFilter>(() => {
    const v = readItem(KEY);
    return v === "agent" || v === "human" ? v : "both";
  });
  const choose = (f: SourceFilter) => {
    setFilter(f);
    storeItem(KEY, f);
  };
  return [filter, choose];
}

export function SourceFilterPills({
  value,
  onChange,
  disabled = false,
}: {
  value: SourceFilter;
  onChange: (f: SourceFilter) => void;
  /** Shown but inactive (for example while editing), so the toolbar never shifts. */
  disabled?: boolean;
}) {
  const t = useT().human;
  return (
    <div className="pill-group icon-pills source-filter" role="radiogroup" aria-label={t.filterTitle}>
      {FILTERS.map((f) => {
        const Icon = ICONS[f];
        return (
          <button
            key={f}
            type="button"
            role="radio"
            className={`pill ${value === f ? "active" : ""}`}
            aria-checked={value === f}
            aria-label={t.filters[f]}
            data-tip={t.filters[f]}
            disabled={disabled}
            onClick={() => onChange(f)}
          >
            <Icon />
          </button>
        );
      })}
    </div>
  );
}
