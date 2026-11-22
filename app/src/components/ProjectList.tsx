import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useT } from "../i18n";
import type { ProjectSummary } from "../types";

/**
 * Sidebar project list. Right-click a project for a context menu; "Rename"
 * turns the row into an inline input (Enter saves, Escape cancels). Renaming
 * also rewrites the project's headings in saved reports on the backend.
 * Drag a row up or down to change the project order (pointer events rather than
 * HTML5 drag and drop, which the desktop webview does not handle reliably).
 */

const DRAG_THRESHOLD_PX = 4;

interface DragState {
  id: number;
  startY: number;
  active: boolean;
  /** Insertion index into the list (0..=length), before removing the dragged row. */
  target: number;
}

/** Move `id` to the insertion `target` computed against the original list. */
function moveTo(ids: number[], id: number, target: number): number[] {
  const from = ids.indexOf(id);
  const rest = ids.filter((x) => x !== id);
  const at = target > from ? target - 1 : target;
  rest.splice(at, 0, id);
  return rest;
}

interface ProjectListProps {
  projects: ProjectSummary[];
  selectedProjectId: number | null;
  onSelectProject: (id: number) => void;
  /** Rejects with the backend error (e.g. a duplicate name) so it can be shown inline. */
  onRenameProject: (id: number, newName: string) => Promise<void>;
  onReorderProjects: (ids: number[]) => Promise<void>;
  onSetArchived: (id: number, archived: boolean) => void;
}

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function ProjectList({
  projects,
  selectedProjectId,
  onSelectProject,
  onRenameProject,
  onReorderProjects,
  onSetArchived,
}: ProjectListProps) {
  const tr = useT();
  const t = tr.projectList;
  const [menu, setMenu] = useState<{ id: number; x: number; y: number } | null>(
    null,
  );
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Drag-to-reorder. `order` shows the new order right away, until the
  // refreshed `projects` prop arrives.
  const [order, setOrder] = useState<number[] | null>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const rowRefs = useRef(new Map<number, HTMLElement>());
  const suppressClick = useRef(false);
  useEffect(() => setOrder(null), [projects]);

  const byId = new Map(projects.map((p) => [p.id, p]));
  const shown = (order ?? projects.map((p) => p.id))
    .map((id) => byId.get(id))
    .filter((p): p is ProjectSummary => p !== undefined);

  useEffect(() => {
    if (!drag) return;
    const onMove = (e: MouseEvent) => {
      setDrag((d) => {
        if (!d) return d;
        const active = d.active || Math.abs(e.clientY - d.startY) > DRAG_THRESHOLD_PX;
        if (!active) return d;
        let target = 0;
        for (const p of shown) {
          const el = rowRefs.current.get(p.id);
          if (!el) continue;
          const r = el.getBoundingClientRect();
          if (e.clientY > r.top + r.height / 2) target += 1;
        }
        return { ...d, active, target };
      });
    };
    const onUp = () => {
      const d = drag;
      setDrag(null);
      if (!d.active) return;
      suppressClick.current = true;
      const ids = shown.map((p) => p.id);
      const next = moveTo(ids, d.id, d.target);
      if (next.some((id, i) => id !== ids[i])) {
        setOrder(next);
        void onReorderProjects(next).catch(() => setOrder(null));
      }
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [drag, shown, onReorderProjects]);

  // Close the context menu on any outside click, scroll, resize or Escape.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  useEffect(() => {
    if (renamingId !== null) inputRef.current?.select();
  }, [renamingId]);

  const startRename = (p: ProjectSummary) => {
    setMenu(null);
    setRenamingId(p.id);
    setDraft(p.name);
    setError(null);
  };

  const cancelRename = () => {
    setRenamingId(null);
    setError(null);
  };

  const commitRename = async (p: ProjectSummary) => {
    const name = draft.trim();
    if (!name || name === p.name) {
      cancelRename();
      return;
    }
    if (
      projects.some(
        (o) => o.id !== p.id && o.name.toLowerCase() === name.toLowerCase(),
      )
    ) {
      setError(t.exists(name));
      return;
    }
    setSaving(true);
    try {
      await onRenameProject(p.id, name);
      cancelRename();
    } catch (e) {
      setError(errText(e));
    } finally {
      setSaving(false);
    }
  };

  if (projects.length === 0) {
    return <div className="muted-line">{t.empty}</div>;
  }

  const menuProject = menu ? projects.find((p) => p.id === menu.id) : undefined;

  return (
    <>
      {shown.map((p, i) =>
        renamingId === p.id ? (
          <div key={p.id} className="project-rename">
            <input
              ref={inputRef}
              type="text"
              value={draft}
              disabled={saving}
              aria-label={t.nameLabel}
              onChange={(e) => {
                setDraft(e.target.value);
                setError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") void commitRename(p);
                if (e.key === "Escape") cancelRename();
              }}
              onBlur={() => {
                if (!saving && !error) void commitRename(p);
              }}
            />
            {error && <div className="project-rename-error">{error}</div>}
          </div>
        ) : (
          <button
            key={p.id}
            ref={(el) => {
              if (el) rowRefs.current.set(p.id, el);
              else rowRefs.current.delete(p.id);
            }}
            type="button"
            className={[
              "day-row",
              p.id === selectedProjectId ? "active" : "",
              drag?.active && drag.id === p.id ? "dragging" : "",
              drag?.active && drag.target === i ? "drop-before" : "",
              drag?.active && i === shown.length - 1 && drag.target === shown.length
                ? "drop-after"
                : "",
            ]
              .filter(Boolean)
              .join(" ")}
            onMouseDown={(e) => {
              if (e.button !== 0 || shown.length < 2) return;
              setDrag({ id: p.id, startY: e.clientY, active: false, target: i });
            }}
            onClick={() => {
              if (suppressClick.current) {
                suppressClick.current = false;
                return;
              }
              onSelectProject(p.id);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ id: p.id, x: e.clientX, y: e.clientY });
            }}
          >
            <span className="day-row-date">{p.name}</span>
            <span className="day-row-meta">
              <span>{p.slices}</span>
            </span>
          </button>
        ),
      )}
      {menu &&
        menuProject &&
        createPortal(
          <div
            className="context-menu"
            role="menu"
            style={{ top: menu.y, left: menu.x }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              role="menuitem"
              className="dropdown-item"
              onClick={() => startRename(menuProject)}
            >
              {tr.common.rename}
            </button>
            <button
              type="button"
              role="menuitem"
              className="dropdown-item"
              onClick={() => {
                setMenu(null);
                onSetArchived(menuProject.id, !menuProject.archived);
              }}
            >
              {menuProject.archived ? tr.projectArchive.restore : tr.projectArchive.archive}
            </button>
          </div>,
          document.body,
        )}
    </>
  );
}
