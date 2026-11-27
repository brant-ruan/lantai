// The "more" menu of the project view, the same on every project tab: project details, the
// attribution rules, items of the current tab, archiving and deleting the project. Also the
// banner an archived project shows on every tab.
import type { ReactNode } from "react";
import { useT } from "../../i18n";
import type { ProjectManage } from "../../hooks/useProjectView";
import { MoreMenu } from "../DropdownMenu";

export interface ProjectMenuActions {
  rulesCount: number;
  archived: boolean;
  onManage: (m: ProjectManage) => void;
  onSetArchived: (archived: boolean) => void;
  /** Asks for confirmation first. */
  onDelete: () => void;
}

export function ArchivedBanner({ actions }: { actions: ProjectMenuActions }) {
  const t = useT().projectArchive;
  if (!actions.archived) return null;
  return (
    <div className="banner banner-info archived-banner">
      <span>{t.banner}</span>
      <button
        type="button"
        className="btn btn-secondary btn-xs"
        onClick={() => actions.onSetArchived(false)}
      >
        {t.restore}
      </button>
    </div>
  );
}

export function ProjectMenu({
  actions,
  children,
}: {
  actions: ProjectMenuActions;
  /** Items of the current tab, shown above the separator. */
  children?: (close: () => void) => ReactNode;
}) {
  const tr = useT();
  const t = tr.timeline;
  return (
    <MoreMenu tip={tr.report.options}>
      {(close) => (
        <>
          {children?.(close)}
          <button
            type="button"
            className="dropdown-item"
            onClick={() => {
              close();
              actions.onManage("details");
            }}
          >
            {t.editDetails}
          </button>
          <button
            type="button"
            className="dropdown-item"
            onClick={() => {
              close();
              actions.onManage("rules");
            }}
          >
            {tr.workspace.rules(actions.rulesCount)}
          </button>
          <div className="dropdown-sep" />
          <button
            type="button"
            className="dropdown-item"
            onClick={() => {
              close();
              actions.onSetArchived(!actions.archived);
            }}
          >
            {actions.archived ? tr.projectArchive.restore : tr.projectArchive.archive}
          </button>
          <button
            type="button"
            className="dropdown-item danger"
            onClick={() => {
              close();
              actions.onDelete();
            }}
          >
            {t.deleteProject}
          </button>
        </>
      )}
    </MoreMenu>
  );
}
