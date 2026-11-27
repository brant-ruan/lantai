// Which note blocks are collapsed. Blocks start expanded; the ones a viewer folds are kept in
// local storage on this machine (a reading convenience, not part of the journal).
import { useCallback, useSyncExternalStore } from "react";
import { readItem, storeItem } from "../storage";

const KEY = "lantai.human.collapsed";

function load(): Set<number> {
  try {
    const raw = readItem(KEY);
    const ids = raw ? (JSON.parse(raw) as unknown) : [];
    return new Set(Array.isArray(ids) ? ids.filter((x): x is number => typeof x === "number") : []);
  } catch {
    return new Set();
  }
}

let collapsed = load();
const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function isNoteCollapsed(id: number): boolean {
  return collapsed.has(id);
}

export function setNoteCollapsed(id: number, value: boolean) {
  setNotesCollapsed([id], value);
}

/** Fold or unfold several blocks at once (one write, one notification). */
export function setNotesCollapsed(ids: readonly number[], value: boolean) {
  const changed = ids.filter((id) => collapsed.has(id) !== value);
  if (changed.length === 0) return;
  collapsed = new Set(collapsed);
  for (const id of changed) {
    if (value) collapsed.add(id);
    else collapsed.delete(id);
  }
  storeItem(KEY, JSON.stringify([...collapsed]));
  listeners.forEach((l) => l());
}

/** Whether block `id` is collapsed, and a setter. */
export function useNoteCollapse(id: number): [boolean, (value: boolean) => void] {
  const value = useSyncExternalStore(subscribe, () => collapsed.has(id));
  const set = useCallback((v: boolean) => setNoteCollapsed(id, v), [id]);
  return [value, set];
}

/** Whether none of the blocks `ids` is collapsed. */
export function useNotesExpanded(ids: readonly number[]): boolean {
  return useSyncExternalStore(subscribe, () => ids.every((id) => !collapsed.has(id)));
}
