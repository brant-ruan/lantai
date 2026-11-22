// Number formatting shared by the statistics dashboard sections.

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
