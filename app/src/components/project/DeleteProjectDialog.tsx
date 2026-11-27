// Confirmation before deleting a project: lists what the deletion takes with it and offers
// archiving instead (or nothing more, when the project is already archived).

import { useEffect, useState } from "react";
import { api } from "../../api";
import { ConfirmModal } from "../ConfirmModal";
import { useT } from "../../i18n";
import type { ProjectDeleteImpact } from "../../types";
import { errorText } from "../../errors";

export interface DeleteTarget {
  id: number;
  name: string;
  archived: boolean;
}

export function DeleteProjectDialog({
  target,
  onDelete,
  onArchive,
  onClose,
}: {
  target: DeleteTarget;
  onDelete: () => void;
  onArchive: () => void;
  onClose: () => void;
}) {
  const tr = useT();
  const t = tr.projectArchive;
  const [impact, setImpact] = useState<ProjectDeleteImpact | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .projectDeleteImpact(target.id)
      .then((i) => live && setImpact(i))
      .catch((e) => live && setError(errorText(e)));
    return () => {
      live = false;
    };
  }, [target.id]);

  const lines = impact
    ? [
        impact.sessions > 0 && t.deleteSessions(impact.sessions),
        impact.reports > 0 && t.deleteReports(impact.reports),
        impact.rules > 0 && t.deleteRules(impact.rules),
        impact.notes > 0 && t.deleteNotes(impact.notes),
      ].filter((l): l is string => !!l)
    : [];

  return (
    <ConfirmModal
      title={t.deleteTitle(target.name)}
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>
            {tr.common.cancel}
          </button>
          {!target.archived && (
            <button type="button" className="btn btn-secondary btn-sm" onClick={onArchive}>
              {t.archiveInstead}
            </button>
          )}
          <button
            type="button"
            className="btn btn-danger btn-sm"
            disabled={!impact}
            onClick={onDelete}
          >
            {t.confirmDelete}
          </button>
        </>
      }
    >
      <p className="confirm-lead">{t.deleteLead}</p>
      {error && <div className="banner banner-err">{error}</div>}
      {lines.length > 0 && (
        <ul className="confirm-list">
          {lines.map((l) => (
            <li key={l}>{l}</li>
          ))}
        </ul>
      )}
      <p className="muted-line">{t.deleteKept}</p>
      {!target.archived && <p className="muted-line">{t.archiveHint}</p>}
    </ConfirmModal>
  );
}
