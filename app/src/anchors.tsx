import React, { useEffect, useMemo, useState } from "react";
import { useT } from "./i18n";
import type { LineEdit, ReportEdits } from "./types";

const ANCHOR_TOKEN_RE =
  /(\[#?S\d+(?:\.T\d+(?:\s*,\s*(?:#?S\d+\.)?\.?T?\d+)*)?\])/g;
const ANCHOR_INNER_RE = /^\[#?S(\d+)([^\]]*)\]$/;
const INLINE_RE =
  /(\*\*[^*]+\*\*|`[^`]+`|\[#?S\d+(?:\.T\d+(?:\s*,\s*(?:#?S\d+\.)?\.?T?\d+)*)?\])/g;
const BULLET_RE = /^([ \t]*)(?:[-*]|\u2022) (.+)$/;

export type AnchorClickHandler = (sliceId: number, turnIndex?: number) => void;

export interface OutlineExpandState {
  hasNested: boolean;
  allCollapsed: boolean;
  allExpanded: boolean;
}

export interface MarkdownReportProps {
  markdown: string;
  onSelectAnchor: AnchorClickHandler;
  expandAll?: boolean;
  expandSeq?: number;
  /** Rendered at the right end of the document title row. */
  headerAction?: React.ReactNode;
  /** Rendered at the right end of each `## <section>` heading row. */
  sectionAction?: (heading: string) => React.ReactNode;
  onExpandStateChange?: (state: OutlineExpandState) => void;
  /** Lines the user wrote, to be marked; line numbers index `markdown.split("\n")`. */
  edits?: ReportEdits | null;
  /** Rendered under the point on a line (before its sub-points): annotations. */
  pointExtras?: (line: number) => React.ReactNode;
  /** Rendered at the right end of a point's row on hover. */
  pointAction?: (line: number) => React.ReactNode;
  /** Rendered at the start of each `## <section>` card's body, before its points. */
  sectionTop?: (heading: string, headingLine: number) => React.ReactNode;
  /** Rendered at the end of each `## <section>` card. */
  sectionFooter?: (heading: string, headingLine: number) => React.ReactNode;
  /** Rendered after the last section. */
  afterSections?: React.ReactNode;
  /** Rendered between the title and the first section. */
  beforeSections?: React.ReactNode;
}

/** Hooks for showing human content inside the generated outline. */
interface OutlineSlots {
  pointExtras?: (line: number) => React.ReactNode;
  pointAction?: (line: number) => React.ReactNode;
}

/** The user's edits keyed by line number. */
interface EditIndex {
  lines: Map<number, LineEdit>;
  removed: Map<number, string[]>;
}

interface OutlineNode {
  id: string;
  /** Line number in the Markdown source. */
  line: number;
  text: string;
  indent: number;
  children: OutlineNode[];
}

type MdBlock =
  | { type: "h1" | "h2" | "h3" | "p" | "quote"; key: string; line: number; text: string }
  | { type: "hr"; key: string }
  | { type: "list"; key: string; items: OutlineNode[] };

interface ReportSection {
  key: string;
  heading: string | null;
  headingLine: number | null;
  blocks: MdBlock[];
}

export interface ParsedAnchorItem {
  sliceId: number;
  turnIndex?: number;
  label: string;
}

export function parseAnchorToken(token: string): ParsedAnchorItem[] | null {
  const m = ANCHOR_INNER_RE.exec(token);
  if (!m) return null;
  const baseSliceId = Number(m[1]);
  const tail = m[2].trim();
  if (!tail) {
    return [{ sliceId: baseSliceId, label: `[#S${baseSliceId}]` }];
  }
  if (!tail.startsWith(".T")) return null;
  const parts = tail.slice(1).split(",");
  const items: ParsedAnchorItem[] = [];
  for (const rawPart of parts) {
    const part = rawPart.trim();
    const refMatch = /^(?:#?S(\d+)\.)?\.?T?(\d+)$/.exec(part);
    if (!refMatch) return null;
    const sliceId = refMatch[1] ? Number(refMatch[1]) : baseSliceId;
    const turnIndex = Number(refMatch[2]);
    items.push({
      sliceId,
      turnIndex,
      label: `[#S${sliceId}.T${turnIndex}]`,
    });
  }
  return items;
}

export function AnchorChip({
  item,
  onSelectAnchor,
}: {
  item: ParsedAnchorItem;
  onSelectAnchor: AnchorClickHandler;
}) {
  const t = useT().anchors;
  return (
    <button
      type="button"
      className="anchor-chip"
      title={
        item.turnIndex !== undefined
          ? t.openTurn(item.sliceId, item.turnIndex)
          : t.openSlice(item.sliceId)
      }
      onClick={() => onSelectAnchor(item.sliceId, item.turnIndex)}
    >
      {item.label}
    </button>
  );
}

function renderAnchorChips(
  items: ParsedAnchorItem[],
  onSelectAnchor: AnchorClickHandler,
  keyPrefix: string,
): React.ReactNode {
  return items.map((item, idx) => (
    <React.Fragment key={`${keyPrefix}-${idx}`}>
      {idx > 0 ? " " : null}
      <AnchorChip item={item} onSelectAnchor={onSelectAnchor} />
    </React.Fragment>
  ));
}

function renderInline(
  text: string,
  onSelectAnchor: AnchorClickHandler,
  keyPrefix: string,
): React.ReactNode[] {
  const parts = text.split(INLINE_RE);
  return parts.map((part, idx) => {
    const key = `${keyPrefix}-${idx}`;
    const anchors = parseAnchorToken(part);
    if (anchors) {
      return (
        <React.Fragment key={key}>
          {renderAnchorChips(anchors, onSelectAnchor, key)}
        </React.Fragment>
      );
    }
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return (
        <strong key={key}>
          {renderAnchorTokens(part.slice(2, -2), onSelectAnchor, key)}
        </strong>
      );
    }
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return <code key={key}>{part.slice(1, -1)}</code>;
    }
    return <React.Fragment key={key}>{part}</React.Fragment>;
  });
}

function renderAnchorTokens(
  text: string,
  onSelectAnchor: AnchorClickHandler,
  keyPrefix: string,
): React.ReactNode[] {
  return text.split(ANCHOR_TOKEN_RE).map((part, idx) => {
    const anchors = parseAnchorToken(part);
    if (!anchors) {
      return (
        <React.Fragment key={`${keyPrefix}-t-${idx}`}>{part}</React.Fragment>
      );
    }
    return (
      <React.Fragment key={`${keyPrefix}-a-${idx}`}>
        {renderAnchorChips(anchors, onSelectAnchor, `${keyPrefix}-a-${idx}`)}
      </React.Fragment>
    );
  });
}

function parseBlocks(markdown: string): {
  blocks: MdBlock[];
  collapsibleIds: string[];
} {
  const lines = markdown.split("\n");
  const blocks: MdBlock[] = [];
  const collapsibleIds: string[] = [];

  let pendingList: Array<{ id: string; line: number; indent: number; text: string }> = [];

  const flushList = (key: string) => {
    if (pendingList.length === 0) return;
    const roots: OutlineNode[] = [];
    const stack: OutlineNode[] = [];

    for (const item of pendingList) {
      const node: OutlineNode = {
        id: item.id,
        line: item.line,
        text: item.text,
        indent: item.indent,
        children: [],
      };
      while (
        stack.length > 0 &&
        stack[stack.length - 1].indent >= item.indent
      ) {
        stack.pop();
      }
      if (stack.length === 0) {
        roots.push(node);
      } else {
        const parent = stack[stack.length - 1];
        if (parent.children.length === 0) {
          collapsibleIds.push(parent.id);
        }
        parent.children.push(node);
      }
      stack.push(node);
    }

    blocks.push({ type: "list", key, items: roots });
    pendingList = [];
  };

  lines.forEach((rawLine, idx) => {
    const trimmed = rawLine.trim();
    if (trimmed === "") {
      return;
    }
    const bullet = BULLET_RE.exec(rawLine);
    if (bullet) {
      const indent = bullet[1].replace(/\t/g, "  ").length;
      pendingList.push({
        id: `n-${idx}`,
        line: idx,
        indent,
        text: bullet[2],
      });
      return;
    }
    flushList(`list-before-${idx}`);

    const line = idx;
    if (trimmed.startsWith("### ")) {
      blocks.push({ type: "h3", key: `h3-${idx}`, line, text: trimmed.slice(4) });
    } else if (trimmed.startsWith("## ")) {
      blocks.push({ type: "h2", key: `h2-${idx}`, line, text: trimmed.slice(3) });
    } else if (trimmed.startsWith("# ")) {
      blocks.push({ type: "h1", key: `h1-${idx}`, line, text: trimmed.slice(2) });
    } else if (trimmed.startsWith("> ")) {
      blocks.push({ type: "quote", key: `q-${idx}`, line, text: trimmed.slice(2) });
    } else if (trimmed === "---") {
      blocks.push({ type: "hr", key: `hr-${idx}` });
    } else {
      blocks.push({ type: "p", key: `p-${idx}`, line, text: trimmed });
    }
  });
  flushList("list-end");

  return { blocks, collapsibleIds };
}

/**
 * Returns whether the given report Markdown contains multi-level collapsible bullets.
 */
export function hasCollapsibleItems(markdown: string): boolean {
  return parseBlocks(markdown).collapsibleIds.length > 0;
}

function groupSections(blocks: MdBlock[]): {
  h1: string | null;
  sections: ReportSection[];
} {
  let h1: string | null = null;
  const sections: ReportSection[] = [];
  let current: ReportSection | null = null;

  for (const block of blocks) {
    if (
      block.type === "h1" &&
      h1 === null &&
      sections.length === 0 &&
      !current
    ) {
      h1 = block.text;
      continue;
    }
    if (block.type === "h2") {
      if (current) {
        sections.push(current);
      }
      current = {
        key: block.key,
        heading: block.text,
        headingLine: block.line,
        blocks: [],
      };
      continue;
    }
    if (!current) {
      current = {
        key: `sec-${block.key}`,
        heading: null,
        headingLine: null,
        blocks: [],
      };
    }
    current.blocks.push(block);
  }

  if (current) {
    sections.push(current);
  }

  return { h1, sections };
}

/** Whether any node below `node` is a line the user wrote. */
function hasEditedDescendant(node: OutlineNode, edits: EditIndex): boolean {
  return node.children.some(
    (c) => edits.lines.has(c.line) || hasEditedDescendant(c, edits),
  );
}

/** Tooltip for a line the user wrote. */
function editTitle(
  edit: LineEdit,
  t: { added: string; rewrote: (original: string) => string },
): string {
  return edit.original === null ? t.added : t.rewrote(edit.original);
}

function OutlineBranch({
  nodes,
  depth,
  expandAll,
  expandedIds,
  onToggle,
  onSelectAnchor,
  edits,
  slots,
}: {
  nodes: OutlineNode[];
  depth: number;
  expandAll: boolean;
  expandedIds: Set<string>;
  onToggle: (id: string) => void;
  onSelectAnchor: AnchorClickHandler;
  edits: EditIndex | null;
  slots: OutlineSlots;
}) {
  const all = useT();
  const t = all.anchors;
  const te = all.edits;
  return (
    <ul
      className={depth === 0 ? "md-list md-outline-root" : "md-list md-sublist"}
    >
      {nodes.map((node) => {
        const hasChildren = node.children.length > 0;
        const isExpanded = expandAll || expandedIds.has(node.id);
        const edit = edits?.lines.get(node.line);
        const editsInside =
          !!edits && hasChildren && !isExpanded && hasEditedDescendant(node, edits);
        return (
          <li key={node.id} className="outline-item">
            <div
              className={`outline-row${edit ? " is-human" : ""}`}
              title={edit ? editTitle(edit, te) : undefined}
            >
              {hasChildren ? (
                <button
                  type="button"
                  className={`outline-toggle ${isExpanded ? "expanded" : ""}`}
                  aria-expanded={isExpanded}
                  title={isExpanded ? t.collapse : t.expand}
                  onClick={() => onToggle(node.id)}
                >
                  <svg
                    width="14"
                    height="14"
                    viewBox="0 0 16 16"
                    fill="none"
                    aria-hidden="true"
                  >
                    <path
                      d="M5.25 3.25L11.25 8L5.25 12.75V3.25Z"
                      fill="currentColor"
                    />
                  </svg>
                </button>
              ) : (
                <span className="outline-leaf-bullet" aria-hidden="true">
                  <span className="outline-leaf-dot" />
                </span>
              )}
              <div
                className={`outline-text ${hasChildren ? "clickable" : ""}`}
                onClick={
                  hasChildren
                    ? (ev) => {
                        if ((ev.target as HTMLElement).closest("button, a")) {
                          return;
                        }
                        onToggle(node.id);
                      }
                    : undefined
                }
              >
                {renderInline(node.text, onSelectAnchor, node.id)}
                {editsInside && (
                  <span className="human-inside-dot" title={te.inside} />
                )}
              </div>
              {slots.pointAction && (
                <div className="outline-row-action">{slots.pointAction(node.line)}</div>
              )}
            </div>
            {slots.pointExtras?.(node.line)}

            {hasChildren && isExpanded && (
              <OutlineBranch
                nodes={node.children}
                depth={depth + 1}
                expandAll={expandAll}
                expandedIds={expandedIds}
                onToggle={onToggle}
                onSelectAnchor={onSelectAnchor}
                edits={edits}
                slots={slots}
              />
            )}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * Render generated or edited report Markdown as an interactive collapsible outliner
 * with clickable `[#S12]` and `[#S12.T5]` source anchors. Defaults to top-level (L2)
 * overview when nested details are present.
 */
export function MarkdownReport({
  markdown,
  onSelectAnchor,
  expandAll: expandAllProp,
  expandSeq = 0,
  headerAction,
  sectionAction,
  onExpandStateChange,
  edits,
  pointExtras,
  pointAction,
  sectionTop,
  sectionFooter,
  afterSections,
  beforeSections,
}: MarkdownReportProps) {
  const slots: OutlineSlots = { pointExtras, pointAction };
  const te = useT().edits;
  const { blocks, collapsibleIds } = useMemo(
    () => parseBlocks(markdown),
    [markdown],
  );
  const { h1, sections } = useMemo(() => groupSections(blocks), [blocks]);
  const editIndex = useMemo<EditIndex | null>(
    () =>
      edits
        ? {
            lines: new Map(edits.lines.map((e) => [e.line, e])),
            removed: new Map(edits.removed.map((r) => [r.headingLine, r.lines])),
          }
        : null,
    [edits],
  );

  const [expandAll, setExpandAll] = useState(Boolean(expandAllProp));
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    const initial = Boolean(expandAllProp);
    setExpandAll(initial);
    setExpandedIds(initial ? new Set(collapsibleIds) : new Set());
  }, [markdown, expandSeq, expandAllProp, collapsibleIds]);

  const hasNested = collapsibleIds.length > 0;
  const allExpanded =
    hasNested &&
    (expandAll || collapsibleIds.every((id) => expandedIds.has(id)));
  const allCollapsed = !expandAll && expandedIds.size === 0;

  useEffect(() => {
    onExpandStateChange?.({
      hasNested,
      allCollapsed,
      allExpanded,
    });
  }, [hasNested, allCollapsed, allExpanded, onExpandStateChange]);

  const handleToggleNode = (id: string) => {
    if (expandAll) {
      const next = new Set(collapsibleIds);
      next.delete(id);
      setExpandAll(false);
      setExpandedIds(next);
      return;
    }
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const renderBlock = (b: MdBlock) => {
    if (b.type === "list") {
      return (
        <OutlineBranch
          key={b.key}
          nodes={b.items}
          depth={0}
          expandAll={expandAll}
          expandedIds={expandedIds}
          onToggle={handleToggleNode}
          onSelectAnchor={onSelectAnchor}
          edits={editIndex}
          slots={slots}
        />
      );
    }
    if (b.type === "h1") {
      return <h1 key={b.key}>{renderInline(b.text, onSelectAnchor, b.key)}</h1>;
    }
    if (b.type === "h2") {
      return <h2 key={b.key}>{renderInline(b.text, onSelectAnchor, b.key)}</h2>;
    }
    if (b.type === "h3") {
      return <h3 key={b.key}>{renderInline(b.text, onSelectAnchor, b.key)}</h3>;
    }
    if (b.type === "hr") {
      return <hr key={b.key} />;
    }
    if (b.type === "quote") {
      return (
        <p key={b.key} className="report-quote">
          {renderInline(b.text, onSelectAnchor, b.key)}
        </p>
      );
    }
    const edit = editIndex?.lines.get(b.line);
    return (
      <p
        key={b.key}
        className={edit ? "is-human" : undefined}
        title={edit ? editTitle(edit, te) : undefined}
      >
        {renderInline(b.text, onSelectAnchor, b.key)}
      </p>
    );
  };

  const formattedTitle = h1;

  return (
    <div className="markdown-body">
      {(formattedTitle || headerAction) && (
        <header className="report-doc-header">
          {formattedTitle && (
            <h1 className="report-doc-title">
              {renderInline(formattedTitle, onSelectAnchor, "doc-h1")}
            </h1>
          )}
          {headerAction && (
            <div className="report-doc-actions">{headerAction}</div>
          )}
        </header>
      )}

      <div className="report-sections">
        {beforeSections}
        {sections.map((sec) =>
          sec.heading ? (
            <section key={sec.key} className="report-section-card">
              <div className="report-section-head">
                <span
                  className={`section-accent-bar ${
                    sec.heading.toLowerCase() === "unassigned"
                      ? "is-unassigned"
                      : ""
                  }`}
                />
                <h2>{renderInline(sec.heading, onSelectAnchor, sec.key)}</h2>
                {(() => {
                  const removed =
                    sec.headingLine !== null
                      ? editIndex?.removed.get(sec.headingLine)
                      : undefined;
                  return removed ? (
                    <span
                      className="human-removed-badge"
                      title={te.removedTitle(removed.join("\n"))}
                    >
                      {te.removed(removed.length)}
                    </span>
                  ) : null;
                })()}
                {sectionAction && (
                  <div className="report-section-action">
                    {sectionAction(sec.heading)}
                  </div>
                )}
              </div>
              <div className="report-section-body">
                {sectionTop && sec.headingLine !== null &&
                  sectionTop(sec.heading, sec.headingLine)}
                {sec.blocks.map(renderBlock)}
                {sectionFooter && sec.headingLine !== null &&
                  sectionFooter(sec.heading, sec.headingLine)}
              </div>
            </section>
          ) : (
            <div key={sec.key} className="report-loose-block">
              {sec.blocks.map(renderBlock)}
            </div>
          ),
        )}
        {afterSections}
      </div>
    </div>
  );
}
