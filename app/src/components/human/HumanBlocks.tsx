// An editable outline of human notes. Each block shows its Markdown rendered and turns into a
// plain text field when clicked. Enter adds a block below (Shift+Enter breaks the line, and
// Enter inside a ``` fence stays in the code), Tab / Shift+Tab indent and outdent, Alt+Up /
// Alt+Down move the block, and Backspace in an empty block removes it. A block with children
// has a triangle that folds them; a folded block opens again when keyboard focus moves inside.
// A block is dragged by its dot or triangle (see `useNoteDrag`); a group given a `zone` is a
// place to drop blocks, outlined while a drag that may land there lasts, and shown as a
// placeholder then even when it has no blocks.
// A block taken off screen while being edited (the report under it was regenerated, so the
// notes are placed again) saves its text, and the same block shown again right away goes on
// editing where it was.
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useT } from "../../i18n";
import type { HumanJournal } from "../../hooks/useHumanJournal";
import type { HumanNode } from "../../types";
import { HumanMarkdown, insideCodeFence, type JournalLinkHandlers } from "./HumanMarkdown";
import { NoteToggle } from "./NoteToggle";
import { useNoteCollapse } from "../../hooks/useNoteCollapse";
import {
  startNoteDrag,
  useNoteDragState,
  zoneAccepts,
  type DropZone,
} from "../../hooks/useNoteDrag";

/** Whether `id` is `root` or lies below it. */
function within(journal: HumanJournal, root: number, id: number): boolean {
  if (id === root) return true;
  return (journal.childrenOf.get(root) ?? []).some((c) => within(journal, c.id, id));
}

/** The block that was being edited when it left the screen, to resume when it comes back. */
let interrupted: { id: number; draft: string; caret: number; at: number } | null = null;
/** How long after leaving the screen a block still resumes editing (it normally comes back in
 *  the same render, or as soon as the reloaded notes arrive). */
const RESUME_MS = 3000;

function takeInterrupted(id: number): { draft: string; caret: number } | null {
  const found = interrupted;
  if (!found || found.id !== id || Date.now() - found.at > RESUME_MS) return null;
  interrupted = null;
  return found;
}

interface BlockProps extends JournalLinkHandlers {
  node: HumanNode;
  journal: HumanJournal;
}

function HumanBlock({ node, journal, ...links }: BlockProps) {
  const t = useT().human;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(node.body);
  // Where to put the caret when editing resumes after the block came back.
  const resumeCaret = useRef<number | null>(null);
  const ref = useRef<HTMLTextAreaElement>(null);
  // Set while a key handler already took care of saving or removing the block.
  const handled = useRef(false);
  // Set when the block leaves the screen, so a blur that follows does not delete or save it.
  const leaving = useRef(false);
  const latest = useRef({ editing, draft, body: node.body, journal });
  latest.current = { editing, draft, body: node.body, journal };

  // The cleanup runs before the text field is removed from the page, so before any blur. The
  // setup runs after every cleanup of the same commit, so a block replaced by a new copy of
  // itself (a remount) finds what the old copy left.
  useLayoutEffect(() => {
    // Reset on every mount: StrictMode runs the cleanup once without removing the block.
    leaving.current = false;
    const resumed = takeInterrupted(node.id);
    if (resumed) {
      resumeCaret.current = resumed.caret;
      setDraft(resumed.draft);
      setEditing(true);
    }
    return () => {
      leaving.current = true;
      const { editing: wasEditing, draft: text, body, journal: j } = latest.current;
      if (!wasEditing) return;
      interrupted = {
        id: node.id,
        draft: text,
        caret: ref.current?.selectionStart ?? text.length,
        at: Date.now(),
      };
      if (text !== body) void j.update(node.id, text);
    };
  }, [node.id]);
  const children = journal.childrenOf.get(node.id) ?? [];
  const [collapsed, setCollapsed] = useNoteCollapse(node.id);
  const drag = useNoteDragState();
  const grab = (e: React.MouseEvent) => startNoteDrag(e, node.id, journal);

  // A block about to take focus (new, moved or indented) must not stay hidden in a fold.
  useEffect(() => {
    const target = journal.focusId;
    if (collapsed && target !== null && target !== node.id && within(journal, node.id, target)) {
      setCollapsed(false);
    }
  }, [collapsed, journal, node.id, setCollapsed]);

  // Follow the stored text while not editing. Only a change of it counts, so a draft restored
  // on mount (see above) is not replaced by the text it was written over.
  const syncedBody = useRef(node.body);
  useEffect(() => {
    if (!editing && node.body !== syncedBody.current) {
      syncedBody.current = node.body;
      setDraft(node.body);
    }
  }, [node.body, editing]);

  useEffect(() => {
    if (journal.focusId === node.id) {
      setEditing(true);
      journal.clearFocus();
    }
  }, [journal.focusId, node.id, journal]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!editing || !el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [editing, draft]);

  useEffect(() => {
    const el = ref.current;
    if (editing && el && document.activeElement !== el) {
      el.focus();
      const caret = Math.min(resumeCaret.current ?? el.value.length, el.value.length);
      resumeCaret.current = null;
      el.setSelectionRange(caret, caret);
    }
  }, [editing]);

  const save = async () => {
    if (draft !== node.body) await journal.update(node.id, draft);
  };

  const onKeyDown = async (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    const el = e.currentTarget;
    if (e.key === "Enter" && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey) {
      if (insideCodeFence(el.value, el.selectionStart)) return;
      e.preventDefault();
      handled.current = true;
      if (draft.trim() === "" && children.length === 0) {
        // Enter on an empty block steps out of its parent, as in other outliners.
        if (node.parentId !== null) await journal.move(node.id, "outdent");
        else handled.current = false;
        return;
      }
      await save();
      await journal.create({ type: "after", id: node.id });
      return;
    }
    if (e.key === "Tab") {
      e.preventDefault();
      handled.current = true;
      await save();
      await journal.move(node.id, e.shiftKey ? "outdent" : "indent");
      return;
    }
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      handled.current = true;
      await save();
      await journal.move(node.id, e.key === "ArrowUp" ? "up" : "down");
      return;
    }
    if (e.key === "Backspace" && draft === "" && children.length === 0) {
      e.preventDefault();
      handled.current = true;
      journal.focusBefore(node.id);
      await journal.remove(node.id);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      el.blur();
    }
  };

  const onBlur = async () => {
    if (leaving.current) return;
    setEditing(false);
    if (handled.current) {
      handled.current = false;
      return;
    }
    if (draft.trim() === "" && children.length === 0) await journal.remove(node.id);
    else await save();
  };

  return (
    <div className={`human-block${node.isAnnotation ? " is-annotation" : ""}`}>
      <div
        className={`human-row${journal.selected.has(node.id) ? " is-selected" : ""}${
          drag?.moving.has(node.id) ? " is-dragging" : ""
        }`}
        data-note-id={node.id}
      >
        {children.length > 0 ? (
          <NoteToggle
            expanded={!collapsed}
            onToggle={() => setCollapsed(!collapsed)}
            onMouseDown={grab}
          />
        ) : (
          <span className="human-bullet" aria-hidden="true" onMouseDown={grab} />
        )}
        {editing ? (
          <textarea
            ref={ref}
            className="human-editor"
            rows={1}
            value={draft}
            placeholder={t.placeholder}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => void onKeyDown(e)}
            onBlur={() => void onBlur()}
          />
        ) : (
          <div
            className="human-text"
            onClick={(e) => {
              if ((e.target as HTMLElement).closest("a, button")) return;
              setEditing(true);
            }}
          >
            {node.body ? (
              <HumanMarkdown body={node.body} {...links} />
            ) : (
              <span className="human-placeholder">{t.placeholder}</span>
            )}
          </div>
        )}
        {!editing && children.length === 0 && (
          <button
            type="button"
            className="human-delete"
            title={t.delete}
            aria-label={t.delete}
            onClick={() => void journal.remove(node.id)}
          >
            ×
          </button>
        )}
      </div>
      {children.length > 0 && !collapsed && (
        <div className="human-children">
          {children.map((c) => (
            <HumanBlock key={c.id} node={c} journal={journal} {...links} />
          ))}
        </div>
      )}
    </div>
  );
}

/** Root blocks of one group (a project's notes, the day's notes, the annotations on a point). */
export function HumanBlocks({
  roots,
  journal,
  zone,
  ...links
}: {
  roots: HumanNode[];
  journal: HumanJournal;
  /** Makes the group a place to drop dragged blocks. */
  zone?: DropZone;
} & JournalLinkHandlers) {
  const t = useT().human;
  const drag = useNoteDragState();
  const target = !!zone && !!drag && zoneAccepts(zone, drag.family);
  const zoneAttr = zone ? JSON.stringify(zone) : undefined;
  if (roots.length === 0) {
    return target ? (
      <div className="human-blocks human-drop-empty is-drop-zone" data-drop-zone={zoneAttr}>
        {t.dropHere}
      </div>
    ) : null;
  }
  return (
    <div
      className={`human-blocks${target ? " is-drop-zone" : ""}`}
      data-drop-zone={zoneAttr}
    >
      {roots.map((n) => (
        <HumanBlock key={n.id} node={n} journal={journal} {...links} />
      ))}
    </div>
  );
}
