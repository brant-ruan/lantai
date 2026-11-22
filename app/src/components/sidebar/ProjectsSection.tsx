// Sidebar project list with an inline form to create a project. Archived projects follow in a
// collapsed group; each part keeps its own drag order.

import { useMemo, useState } from "react";
import { readItem, storeItem } from "../../hooks/storage";
import { useT } from "../../i18n";
import type { ProjectSummary } from "../../types";
import { ProjectList } from "../ProjectList";

const ARCHIVED_OPEN_KEY = "lantai.sidebar.archivedOpen";

export function ProjectsSection({
  projects,
  selectedProjectId,
  onSelectProject,
  onRenameProject,
  onReorderProjects,
  onSetArchived,
  onQuickCreateProject,
}: {
  projects: ProjectSummary[];
  selectedProjectId: number | null;
  onSelectProject: (id: number) => void;
  onRenameProject: (id: number, newName: string) => Promise<void>;
  onReorderProjects: (ids: number[]) => Promise<void>;
  onSetArchived: (id: number, archived: boolean) => void;
  onQuickCreateProject: (name: string, description: string | null) => Promise<void>;
}) {
  const t = useT();
  const [showNew, setShowNew] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDesc, setNewDesc] = useState("");
  // Memoized: ProjectList resets its pending drag order whenever its list changes identity.
  const active = useMemo(() => projects.filter((p) => !p.archived), [projects]);
  const archived = useMemo(() => projects.filter((p) => p.archived), [projects]);
  // The selected project keeps its group open, so it is never hidden.
  const [archivedOpen, setArchivedOpen] = useState(() => readItem(ARCHIVED_OPEN_KEY) === "1");
  const showArchived =
    archivedOpen || archived.some((p) => p.id === selectedProjectId);
  const toggleArchived = () => {
    const next = !showArchived;
    setArchivedOpen(next);
    storeItem(ARCHIVED_OPEN_KEY, next ? "1" : "0");
  };
  const ids = (list: ProjectSummary[]) => list.map((p) => p.id);

  return (
    <div className="sidebar-section days-list-wrap">
      <div className="section-header-row">
        <div className="section-label">{t.sidebar.projects}</div>
        <button
          type="button"
          className="btn btn-ghost btn-xs"
          onClick={() => setShowNew((v) => !v)}
        >
          {t.common.newItem}
        </button>
      </div>
      {showNew && (
        <form
          className="inline-card stack"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!newName.trim()) return;
            await onQuickCreateProject(newName.trim(), newDesc.trim() || null);
            setNewName("");
            setNewDesc("");
            setShowNew(false);
          }}
        >
          <input
            type="text"
            placeholder={t.sidebar.projectName}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            required
          />
          <input
            type="text"
            placeholder={t.sidebar.projectDescription}
            value={newDesc}
            onChange={(e) => setNewDesc(e.target.value)}
          />
          <div className="toolbar-actions">
            <button type="submit" className="btn btn-primary btn-xs">
              {t.common.create}
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              onClick={() => setShowNew(false)}
            >
              {t.common.cancel}
            </button>
          </div>
        </form>
      )}
      <div className="days-list">
        <ProjectList
          projects={active}
          selectedProjectId={selectedProjectId}
          onSelectProject={onSelectProject}
          onRenameProject={onRenameProject}
          onReorderProjects={(order) => onReorderProjects([...order, ...ids(archived)])}
          onSetArchived={onSetArchived}
        />
        {archived.length > 0 && (
          <>
            <button
              type="button"
              className="archived-group-toggle"
              aria-expanded={showArchived}
              onClick={toggleArchived}
            >
              <span className="chevron" aria-hidden="true">
                ›
              </span>
              {t.projectArchive.group(archived.length)}
            </button>
            {showArchived && (
              <div className="archived-list">
                <ProjectList
                  projects={archived}
                  selectedProjectId={selectedProjectId}
                  onSelectProject={onSelectProject}
                  onRenameProject={onRenameProject}
                  onReorderProjects={(order) => onReorderProjects([...ids(active), ...order])}
                  onSetArchived={onSetArchived}
                />
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
