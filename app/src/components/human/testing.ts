// Building human journals for tests: nodes with defaults, and the `JournalNodes` view of them.
import type { HumanNode, HumanPlacement } from "../../types";
import type { JournalNodes } from "./layout";

let nextId = 1;

/** A node of the day `2026-11-16` with defaults; override what the test is about. */
export function node(fields: Partial<HumanNode> & { body: string }): HumanNode {
  const id = fields.id ?? nextId++;
  return {
    id,
    periodKind: "day",
    periodKey: "2026-11-16",
    projectId: null,
    parentId: null,
    sortKey: String(id).padStart(4, "0"),
    isAnnotation: false,
    position: "bottom",
    quote: null,
    createdAt: "2026-11-16T10:00:00Z",
    updatedAt: "2026-11-16T10:00:00Z",
    ...fields,
  };
}

/** The journal of `nodes` (children in the order given) with the annotations' placements. */
export function journalOf(nodes: HumanNode[], placements: HumanPlacement[] = []): JournalNodes {
  const childrenOf = new Map<number | null, HumanNode[]>();
  for (const n of nodes) {
    childrenOf.set(n.parentId, [...(childrenOf.get(n.parentId) ?? []), n]);
  }
  return { childrenOf, placementOf: new Map(placements.map((p) => [p.id, p])) };
}
