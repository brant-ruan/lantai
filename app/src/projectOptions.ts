// Which projects an assignment menu offers.

/** Archived projects are offered only when already chosen, so the menu can show it. */
export function assignableProjects<T extends { id: number; archived?: boolean }>(
  projects: T[],
  current?: number | null,
): T[] {
  return projects.filter((p) => !p.archived || p.id === current);
}
