// Pure date helpers for the sidebar calendar and lists. Dates are local "YYYY-MM-DD",
// months "YYYY-MM", ISO weeks "YYYY-Www".

const pad2 = (n: number) => String(n).padStart(2, "0");

/** "11-17" within the current year, the full date otherwise. */
export function dayRowLabel(ymd: string, today: string): string {
  return today && ymd.slice(0, 4) === today.slice(0, 4) ? ymd.slice(5) : ymd;
}

export function formatMinutes(mins: number): string {
  if (mins < 60) return `${mins}m`;
  const h = mins / 60;
  return `${h >= 10 ? Math.round(h) : h.toFixed(1).replace(/\.0$/, "")}h`;
}

/** ISO week key ("2026-W47") of a "YYYY-MM-DD" date. */
export function isoWeekKey(ymd: string): string {
  const [y, m, d] = ymd.split("-").map(Number);
  const thursday = new Date(Date.UTC(y, m - 1, d));
  thursday.setUTCDate(thursday.getUTCDate() - ((thursday.getUTCDay() + 6) % 7) + 3);
  const isoYear = thursday.getUTCFullYear();
  const week = 1 + Math.floor((thursday.getTime() - Date.UTC(isoYear, 0, 1)) / 86_400_000 / 7);
  return `${isoYear}-W${pad2(week)}`;
}

/** "YYYY-MM" of the Thursday of an ISO week key: the month the week mostly lies in. */
export function weekMonth(key: string): string | null {
  const m = /^(\d{4})-W(\d{2})$/.exec(key);
  if (!m) return null;
  const jan4 = new Date(Date.UTC(Number(m[1]), 0, 4));
  const monday = jan4.getTime() - ((jan4.getUTCDay() + 6) % 7) * 86_400_000;
  const thursday = new Date(monday + ((Number(m[2]) - 1) * 7 + 3) * 86_400_000);
  return `${thursday.getUTCFullYear()}-${pad2(thursday.getUTCMonth() + 1)}`;
}

/** Heat level 0..4 of `minutes` relative to the busiest period. */
export function activityLevel(minutes: number, max: number): number {
  return minutes === 0 ? 0 : Math.min(4, Math.ceil((minutes / max) * 4));
}

export function currentYearMonth(): string {
  const now = new Date();
  return `${now.getFullYear()}-${pad2(now.getMonth() + 1)}`;
}

export function shiftMonth(ym: string, delta: number): string {
  const base = /^\d{4}-\d{2}$/.test(ym) ? ym : currentYearMonth();
  const [y, m] = base.split("-").map(Number);
  const d = new Date(y, m - 1 + delta, 1);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}`;
}

export function shiftYear(yyyy: string, delta: number): string {
  const y = /^\d{4}$/.test(yyyy) ? Number(yyyy) : new Date().getFullYear();
  return String(y + delta);
}

export interface CalendarCell {
  date: string;
  day: number;
  inMonth: boolean;
}

/** The fixed 6x7 grid (Monday first) showing `ym`, padded with neighbouring days. */
export function monthCells(ym: string): CalendarCell[] {
  const [y, m] = ym.split("-").map(Number);
  const leading = (new Date(y, m - 1, 1).getDay() + 6) % 7;
  const cells: CalendarCell[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(y, m - 1, 1 - leading + i);
    cells.push({
      date: `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`,
      day: d.getDate(),
      inMonth: d.getFullYear() === y && d.getMonth() === m - 1,
    });
  }
  return cells;
}
