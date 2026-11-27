// localStorage helpers for per-viewer preferences; storage errors (private mode, quota) are ignored.

export function storeItem(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Ignore storage errors.
  }
}

export function readItem(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** The stored value if it is one of `valid`, else `fallback`. */
export function readChoice<T extends string>(key: string, valid: readonly T[], fallback: T): T {
  const raw = readItem(key) as T | null;
  return raw && valid.includes(raw) ? raw : fallback;
}

/** The stored number clamped to `[min, max]`; `fallback` when unset or not a number. */
export function readNumber(key: string, fallback: number, min: number, max: number): number {
  const raw = readItem(key);
  if (!raw) return fallback;
  const n = Number(raw);
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback;
}
