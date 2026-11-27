import { ConfirmModal } from "../../ConfirmModal";
import { useT } from "../../../i18n";

/** A model alias or provider waiting for the user to confirm its deletion. */
export type PendingDelete =
  | { kind: "model"; name: string }
  | { kind: "provider"; name: string; hasKey: boolean; usedBy: string[] };

/** Confirms deleting a model alias or provider. A provider that models still use cannot be
 * deleted, so the dialog names those models instead. */
export function DeleteConfirm({
  target,
  onClose,
  onDelete,
}: {
  target: PendingDelete;
  onClose: () => void;
  onDelete: () => void;
}) {
  const tr = useT();
  const tm = tr.models;
  if (target.kind === "provider" && target.usedBy.length > 0) {
    return (
      <ConfirmModal
        title={tm.providerInUse(target.name)}
        onClose={onClose}
        actions={
          <button type="button" className="btn btn-secondary btn-sm" onClick={onClose}>
            {tr.common.close}
          </button>
        }
      >
        <p>{tm.usedBy}</p>
        <ul className="confirm-list">
          {target.usedBy.map((alias) => (
            <li key={alias}>{alias}</li>
          ))}
        </ul>
      </ConfirmModal>
    );
  }
  return (
    <ConfirmModal
      title={
        target.kind === "model" ? tm.deleteModel(target.name) : tm.deleteProvider(target.name, target.hasKey)
      }
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>
            {tr.common.cancel}
          </button>
          <button type="button" className="btn btn-danger btn-sm" onClick={onDelete}>
            {tr.common.delete}
          </button>
        </>
      }
    />
  );
}
