// State of the human journal for the view on screen (a report period or a project page): its
// notes and annotations,
// the changes made to them (applied one at a time, in order), which block should take
// keyboard focus next, which blocks are selected, and the deletions that can be undone.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { isNoteCollapsed } from "./useNoteCollapse";
import type {
  HumanDrop,
  HumanMove,
  HumanNewPlace,
  HumanNode,
  HumanPlacement,
  HumanScope,
  HumanView,
  RemovedHumanNode,
} from "../types";


export interface HumanJournal {
  nodes: HumanNode[];
  /** Children of each node, and roots under key `null`, in order. */
  childrenOf: Map<number | null, HumanNode[]>;
  placementOf: Map<number, HumanPlacement>;
  /** The block that should take focus, until it reports it has. */
  focusId: number | null;
  clearFocus: () => void;
  create: (place: HumanNewPlace, body?: string) => Promise<number | null>;
  update: (id: number, body: string) => Promise<void>;
  move: (id: number, direction: HumanMove) => Promise<void>;
  /** Move dragged blocks (each with everything under it) to one place; they stay selected. */
  moveTo: (ids: number[], drop: HumanDrop) => Promise<void>;
  /** Delete blocks with everything under them; the deletion can be undone. */
  remove: (ids: number | number[]) => Promise<void>;
  /** Put back what the last deletion removed; false when there is nothing to undo. */
  undo: () => boolean;
  /** Focus the block shown before `id` (after a delete). */
  focusBefore: (id: number) => void;
  /** Blocks selected by dragging over them (to delete them together). */
  selected: ReadonlySet<number>;
  select: (ids: ReadonlySet<number>) => void;
}

const NONE: ReadonlySet<number> = new Set();
/** Deletions kept for undo, per view on screen. */
const UNDO_DEPTH = 50;

const EMPTY: HumanView = { nodes: [], placements: [] };

/**
 * `reloadKey` changes whenever the report version on screen changes, since annotation
 * placements depend on it. `onCountChanged` runs after a change that may add or remove a
 * top-level note (the sidebar shows their number).
 */
export function useHumanJournal(
  scope: HumanScope,
  reloadKey: string,
  onError: (message: string) => void,
  onCountChanged?: () => void
): HumanJournal {
  const countChanged = useRef(onCountChanged);
  countChanged.current = onCountChanged;
  const [view, setView] = useState<HumanView>(EMPTY);
  const [focusId, setFocusId] = useState<number | null>(null);
  const [selected, setSelected] = useState<ReadonlySet<number>>(NONE);
  const undoStack = useRef<RemovedHumanNode[][]>([]);
  // Changes run one after another so that a structural change never overtakes a pending save.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  // The scope as a value that only changes when the scope does.
  const scopeKey = JSON.stringify(scope);
  const scopeRef = useRef(scopeKey);
  scopeRef.current = scopeKey;
  const target = useMemo(() => JSON.parse(scopeKey) as HumanScope, [scopeKey]);

  const run = useCallback(
    <T,>(task: () => Promise<T>): Promise<T | null> => {
      const started = scopeKey;
      const next = queue.current.then(async () => {
        try {
          return await task();
        } catch (e) {
          onError(String(e));
          return null;
        }
      });
      queue.current = next;
      return next.then((r) => (scopeRef.current === started ? r : null));
    },
    [scopeKey, onError],
  );

  // Reload when the scope or the report version changes. A reload of the same scope keeps the
  // notes on screen until the new ones arrive, so a block being edited is not taken away; it
  // runs in the queue, so it cannot overtake a save that started before it.
  const loadedScope = useRef<string | null>(null);
  useEffect(() => {
    if (loadedScope.current !== scopeKey) {
      loadedScope.current = scopeKey;
      setView(EMPTY);
      setSelected(NONE);
    }
    void run(() => api.humanLoad(target)).then((v) => v && setView(v));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, reloadKey]);

  const create = useCallback(
    async (place: HumanNewPlace, body = "") => {
      const r = await run(() => api.humanCreate(target, place, body));
      if (!r) return null;
      setView(r.view);
      setFocusId(r.id);
      countChanged.current?.();
      return r.id;
    },
    [run, target],
  );

  const update = useCallback(
    async (id: number, body: string) => {
      const v = await run(() => api.humanUpdate(target, id, body));
      if (v) setView(v);
    },
    [run, target],
  );

  const move = useCallback(
    async (id: number, direction: HumanMove) => {
      const v = await run(() => api.humanMove(target, id, direction));
      if (v) {
        setView(v);
        setFocusId(id);
        if (direction === "indent" || direction === "outdent") countChanged.current?.();
      }
    },
    [run, target],
  );

  const moveTo = useCallback(
    async (ids: number[], drop: HumanDrop) => {
      const v = await run(() => api.humanMoveTo(target, ids, drop));
      if (!v) return;
      setView(v);
      setSelected(new Set());
      countChanged.current?.();
    },
    [run, target],
  );

  // A new view starts with nothing to undo.
  useEffect(() => {
    undoStack.current = [];
  }, [scopeKey]);

  const remove = useCallback(
    async (ids: number | number[]) => {
      const list = Array.isArray(ids) ? ids : [ids];
      if (list.length === 0) return;
      const r = await run(() => api.humanDelete(target, list));
      if (!r) return;
      setView(r.view);
      setSelected(NONE);
      if (r.removed.length > 0) {
        undoStack.current = [...undoStack.current.slice(1 - UNDO_DEPTH), r.removed];
      }
      countChanged.current?.();
    },
    [run, target],
  );

  const undo = useCallback(() => {
    const rows = undoStack.current.pop();
    if (!rows) return false;
    void run(() => api.humanRestore(target, rows)).then((v) => {
      if (!v) return;
      setView(v);
      // Select what came back, so it is easy to see (and to delete again).
      const restored = new Set(v.nodes.map((n) => n.id));
      setSelected(new Set(rows.map((r) => r.id).filter((id) => restored.has(id))));
      countChanged.current?.();
    });
    return true;
  }, [run, target]);

  const childrenOf = useMemo(() => {
    const map = new Map<number | null, HumanNode[]>();
    for (const n of view.nodes) {
      const list = map.get(n.parentId) ?? [];
      list.push(n);
      map.set(n.parentId, list);
    }
    for (const list of map.values()) {
      list.sort((a, b) => (a.sortKey < b.sortKey ? -1 : a.sortKey > b.sortKey ? 1 : a.id - b.id));
    }
    return map;
  }, [view.nodes]);

  const placementOf = useMemo(
    () => new Map(view.placements.map((p) => [p.id, p])),
    [view.placements],
  );

  const focusBefore = useCallback(
    (id: number) => {
      const node = view.nodes.find((n) => n.id === id);
      if (!node) return;
      const siblings = (childrenOf.get(node.parentId) ?? []).filter(
        (n) =>
          n.periodKind === node.periodKind &&
          n.periodKey === node.periodKey &&
          n.projectId === node.projectId &&
          n.position === node.position &&
          n.isAnnotation === node.isAnnotation &&
          (!n.isAnnotation ||
            placementOf.get(n.id)?.line === placementOf.get(node.id)?.line),
      );
      const idx = siblings.findIndex((n) => n.id === id);
      let target: HumanNode | undefined = idx > 0 ? siblings[idx - 1] : undefined;
      // The last visible block inside the previous sibling (not inside a folded one).
      while (target) {
        const kids = childrenOf.get(target.id);
        if (!kids || kids.length === 0 || isNoteCollapsed(target.id)) break;
        target = kids[kids.length - 1];
      }
      const fallback = node.parentId;
      setFocusId(target?.id ?? fallback);
    },
    [view.nodes, childrenOf, placementOf],
  );

  return {
    nodes: view.nodes,
    childrenOf,
    placementOf,
    focusId,
    clearFocus: useCallback(() => setFocusId(null), []),
    create,
    update,
    move,
    moveTo,
    remove,
    undo,
    focusBefore,
    selected,
    select: setSelected,
  };
}
