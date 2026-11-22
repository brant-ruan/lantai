// Preferred reading width of report documents, persisted per viewer and shared by every
// report view and the settings page (a tiny external store, so all of them stay in sync).
import { useSyncExternalStore } from "react";
import { readItem, storeItem } from "./storage";

const STORAGE_KEY = "aj-report-width";
export const REPORT_WIDTH_DEFAULT = 860;
export const REPORT_WIDTH_MIN = 560;
// Hard cap so a stored value stays sane on ultra-wide displays.
export const REPORT_WIDTH_MAX = 1600;
/** Step of the slider in Settings. */
export const REPORT_WIDTH_STEP = 20;

function clamp(n: number): number {
  return Math.min(REPORT_WIDTH_MAX, Math.max(REPORT_WIDTH_MIN, Math.round(n)));
}

function load(): number {
  const n = Number(readItem(STORAGE_KEY));
  return readItem(STORAGE_KEY) !== null && Number.isFinite(n) ? clamp(n) : REPORT_WIDTH_DEFAULT;
}

let current = load();
const listeners = new Set<() => void>();

export function setReportWidth(next: number) {
  const value = clamp(next);
  if (value === current) return;
  current = value;
  storeItem(STORAGE_KEY, String(value));
  listeners.forEach((l) => l());
}

export function resetReportWidth() {
  setReportWidth(REPORT_WIDTH_DEFAULT);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useReportWidth(): number {
  return useSyncExternalStore(subscribe, () => current);
}
