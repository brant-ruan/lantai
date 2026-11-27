// Series colours for the statistics dashboard: colour strictly encodes projects (the top five,
// then Other, then striped Unassigned), shared by the trend chart and the project list.

import type { StatsProjectItem } from "../../types";

export interface SeriesInfo {
  key: string;
  label: string;
  color: string;
  svgFill: string;
  isUnassigned: boolean;
  order: number;
}

export interface SeriesLookup {
  resolve: (projectId: number | null) => SeriesInfo;
  /** The series present in the range, in stacking order. */
  legend: SeriesInfo[];
}

const SERIES_COLORS = [
  "var(--series-1)",
  "var(--series-2)",
  "var(--series-3)",
  "var(--series-4)",
  "var(--series-5)",
];

/** The SVG pattern the trend chart defines for unassigned time. */
export const UNASSIGNED_PATTERN_ID = "stats-unassigned-pattern";

export function buildSeriesLookup(
  projects: StatsProjectItem[],
  labels: { other: string; unassigned: string },
): SeriesLookup {
  const topNamed = projects
    .filter((p) => p.projectId !== null && p.activeMinutes > 0)
    .slice(0, 5);
  const byId = new Map<number, SeriesInfo>();
  topNamed.forEach((p, idx) => {
    if (p.projectId === null) return;
    const color = SERIES_COLORS[idx];
    byId.set(p.projectId, {
      key: `p:${p.projectId}`,
      label: p.name,
      color,
      svgFill: color,
      isUnassigned: false,
      order: idx,
    });
  });

  const otherSeries: SeriesInfo = {
    key: "other",
    label: labels.other,
    color: "var(--series-other)",
    svgFill: "var(--series-other)",
    isUnassigned: false,
    order: 5,
  };
  const unassignedSeries: SeriesInfo = {
    key: "unassigned",
    label: labels.unassigned,
    color: "var(--series-unassigned-bg)",
    svgFill: `url(#${UNASSIGNED_PATTERN_ID})`,
    isUnassigned: true,
    order: 6,
  };

  const resolve = (projectId: number | null): SeriesInfo => {
    if (projectId === null) return unassignedSeries;
    return byId.get(projectId) ?? otherSeries;
  };

  const legendMap = new Map<string, SeriesInfo>();
  for (const p of projects) {
    if (p.activeMinutes <= 0) continue;
    const info = resolve(p.projectId);
    if (!legendMap.has(info.key)) legendMap.set(info.key, info);
  }
  const legend = Array.from(legendMap.values()).sort((a, b) => a.order - b.order);

  return { resolve, legend };
}

/** A series' colour chip; unassigned time is striped by CSS. */
export function SeriesSwatch({ info, round = false }: { info: SeriesInfo; round?: boolean }) {
  return (
    <span
      className={`stats-swatch ${round ? "round " : ""}${info.isUnassigned ? "unassigned" : ""}`}
      style={info.isUnassigned ? undefined : { background: info.color }}
    />
  );
}
