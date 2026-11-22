// The one "Expand details / Collapse" control of a report view. It covers whatever the view
// shows that folds: the generated outline's sub-points and the note blocks that have children,
// each counted only while it is shown (the source filter decides) and has something to fold.
import { useMemo } from "react";
import type { HumanJournal } from "./useHumanJournal";
import { setNotesCollapsed, useNotesExpanded } from "./useNoteCollapse";

/** One kind of foldable content in a view. */
export interface FoldPart {
  /** Shown and has something to fold. */
  present: boolean;
  expanded: boolean;
  setExpanded: (value: boolean) => void;
}

export interface DetailsToggle {
  /** Whether there is anything to fold (show the control only then). */
  visible: boolean;
  /** Whether everything shown is unfolded (the control then offers to collapse). */
  expanded: boolean;
  toggle: () => void;
}

/** The note blocks of `journal` that have children, as a fold part shown when `shown`. */
export function useNoteFolds(journal: HumanJournal, shown: boolean): FoldPart {
  const parents = useMemo(
    () => journal.nodes.filter((n) => (journal.childrenOf.get(n.id)?.length ?? 0) > 0).map((n) => n.id),
    [journal.nodes, journal.childrenOf],
  );
  const expanded = useNotesExpanded(parents);
  return {
    present: shown && parents.length > 0,
    expanded,
    setExpanded: (value) => setNotesCollapsed(parents, !value),
  };
}

/** Combine the parts of a view into one control: it unfolds all of them or folds all of them. */
export function detailsToggle(parts: FoldPart[]): DetailsToggle {
  const present = parts.filter((p) => p.present);
  const expanded = present.length > 0 && present.every((p) => p.expanded);
  return {
    visible: present.length > 0,
    expanded,
    toggle: () => present.forEach((p) => p.setExpanded(!expanded)),
  };
}
