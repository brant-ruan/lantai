// Small pieces of the human layer shared by the report views and the project page: the quiet
// "+ Note" buttons (at the end of a notes list; in a card's header, shown on hover, with a menu
// for adding at the top or the bottom of the card), the "Annotate" action on a report point,
// and annotations whose point is gone.
import { DropdownMenu } from "../DropdownMenu";
import { useT } from "../../i18n";
import type { HumanJournal } from "../../hooks/useHumanJournal";
import type { HumanNode, HumanPosition } from "../../types";
import { HumanBlocks } from "./HumanBlocks";
import type { JournalLinkHandlers } from "./HumanMarkdown";

export function AddNoteButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button type="button" className="btn btn-ghost btn-xs human-add" onClick={onClick}>
      + {label}
    </button>
  );
}

/** "+ Note" in the header of a project card or period entry, shown while the pointer is on the
 *  card; its menu adds the note at the top of the card (before the agent's points) or at the
 *  bottom. */
export function HeaderAddNote({ onAdd }: { onAdd: (position: HumanPosition) => void }) {
  const t = useT().human;
  const item = (position: HumanPosition, label: string, close: () => void) => (
    <button
      type="button"
      className="dropdown-item"
      onClick={() => {
        close();
        onAdd(position);
      }}
    >
      {label}
    </button>
  );
  return (
    <DropdownMenu
      label={`+ ${t.addNote}`}
      variant="ghost"
      size="xs"
      hideChevron
      buttonClassName="human-head-add"
    >
      {(close) => (
        <>
          {item("top", t.noteAtTop, close)}
          {item("bottom", t.noteAtBottom, close)}
        </>
      )}
    </DropdownMenu>
  );
}

export function AnnotateButton({ onClick }: { onClick: () => void }) {
  const t = useT().human;
  return (
    <button
      type="button"
      className="btn btn-ghost btn-xs human-annotate"
      title={t.annotateTitle}
      onClick={onClick}
    >
      {t.annotate}
    </button>
  );
}

/** Annotations whose point changed, each with the text it was written on. */
export function OrphanedAnnotations({
  list,
  journal,
  ...links
}: { list: HumanNode[]; journal: HumanJournal } & JournalLinkHandlers) {
  const t = useT().human;
  if (list.length === 0) return null;
  return (
    <div className="human-orphans">
      <div className="human-orphans-title">{t.pointChanged}</div>
      {list.map((n) => (
        <div key={n.id} className="human-orphan">
          {n.quote && <div className="human-quote">{n.quote}</div>}
          <HumanBlocks roots={[n]} journal={journal} {...links} />
        </div>
      ))}
    </div>
  );
}
