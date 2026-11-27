// Number and duration formatting shared by the statistics dashboard sections.

export function formatInt(n: number): string {
  return n.toLocaleString();
}

export function formatPct(pct: number): string {
  if (pct <= 0) return "0%";
  if (pct < 1) return "<1%";
  return `${Math.round(pct)}%`;
}

/** Compact token count: 950, 12.3K, 4.56M, 1.2B. */
export function formatTokens(n: number): string {
  const units: [number, string][] = [
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "K"],
  ];
  for (const [size, suffix] of units) {
    if (n >= size) {
      const v = n / size;
      const digits = v >= 100 ? 0 : v >= 10 ? 1 : 2;
      return `${v.toFixed(digits).replace(/\.?0+$/, "")}${suffix}`;
    }
  }
  return String(n);
}

/** Active minutes as 45m, 2h 30m, 1.5h or 12h. */
export function formatDuration(mins: number): string {
  if (mins <= 0) return "0m";
  if (mins < 60) return `${mins}m`;
  const h = Math.floor(mins / 60);
  const rem = mins % 60;
  if (h >= 10 || rem === 0) {
    const rounded = mins / 60;
    return `${h >= 10 ? Math.round(rounded) : rounded.toFixed(1).replace(/\.0$/, "")}h`;
  }
  return `${h}h ${rem}m`;
}

/** Axis labels: 0, 45m, 1.5h, 12h. */
export function formatAxisDuration(mins: number): string {
  if (mins <= 0) return "0";
  if (mins < 60) return `${mins}m`;
  const h = mins / 60;
  return `${h >= 10 ? Math.round(h) : h.toFixed(1).replace(/\.0$/, "")}h`;
}

export function padHour(h: number): string {
  return String(h).padStart(2, "0");
}
