// The report or project page on screen as Markdown for copying, following the source switch:
// the agent's text and the person's together, the agent's alone, or the person's alone. It is
// laid out like the screen (`layout.ts`). In the combined text everything people wrote is a
// quote, so it stays apart from the agent's points once pasted; in the human-only text the
// agent's points shown as context are the quotes instead.
import type { HumanNode, SourceFilter } from "../../types";
import {
  entryLayout,
  pageNotesOf,
  periodLayout,
  pointText,
  type JournalNodes,
  type ProjectRef,
} from "./layout";

export interface CopyLabels {
  dayNotes: string;
  pageNotes: string;
  otherAnnotations: string;
  pointChanged: string;
}

type Block = string[];

const BULLET_RE = /^([ \t]*)(?:[-*]|•) /;

/** Nodes with everything under them as a Markdown list, `indent` spaces deep. */
function noteList(journal: JournalNodes, roots: HumanNode[], indent = 0): string[] {
  const pad = " ".repeat(indent);
  const out: string[] = [];
  for (const n of roots) {
    const [first, ...rest] = n.body.trimEnd().split("\n");
    out.push(`${pad}- ${first}`.trimEnd());
    // Continuation lines stay inside the item.
    for (const l of rest) out.push(l.trim() === "" ? "" : `${pad}  ${l}`);
    out.push(...noteList(journal, journal.childrenOf.get(n.id) ?? [], indent + 2));
  }
  return out;
}

function quote(lines: string[], indent = 0): string[] {
  const pad = " ".repeat(indent);
  return lines.map((l) => (l === "" ? `${pad}>` : `${pad}> ${l}`));
}

/** A blank line after every heading that has text right under it, as copied text reads best. */
function spaceHeadings(markdown: string): string {
  const lines = markdown.split("\n");
  const out: string[] = [];
  lines.forEach((l, i) => {
    out.push(l);
    const next = lines[i + 1];
    if (/^#{1,6}\s/.test(l) && next !== undefined && next.trim() !== "") out.push("");
  });
  return out.join("\n");
}

/** Compose blocks separated by blank lines, dropping empty ones. */
function join(blocks: Block[]): string {
  const text = blocks
    .map((b) => b.join("\n").replace(/^\n+|\s+$/g, ""))
    .filter((b) => b !== "")
    .join("\n\n");
  return spaceHeadings(text);
}

class Writer {
  constructor(
    private journal: JournalNodes,
    private labels: CopyLabels,
    /** Agent and human together (else only what people wrote). */
    private combined: boolean,
  ) {}

  notes(list: HumanNode[]): Block {
    const lines = noteList(this.journal, list);
    return this.combined ? quote(lines) : lines;
  }

  /** Annotations with the text of the point they are on. */
  withContext(context: string, list: HumanNode[]): Block {
    const lines = noteList(this.journal, list);
    if (context.trim() === "") return this.notes(list);
    return this.combined ? quote([context, "", ...lines]) : [`> ${context}`, "", ...lines];
  }

  /** Annotations whose point is gone, each with the text it was written on. */
  orphans(list: HumanNode[]): Block[] {
    return list.map((n) =>
      this.withContext(
        `*${this.labels.pointChanged}*${n.quote ? `: ${n.quote}` : ""}`,
        [n],
      ),
    );
  }

  /** The agent's lines `from..to`, each point followed by its annotations (quoted, nested). */
  agentLines(
    lines: string[],
    from: number,
    to: number,
    annotationsAt: (line: number) => HumanNode[],
  ): Block {
    const out: string[] = [];
    for (let i = from; i < to; i++) {
      out.push(lines[i]);
      const bullet = BULLET_RE.exec(lines[i]);
      const list = bullet ? annotationsAt(i) : [];
      if (bullet && list.length > 0) {
        const indent = bullet[1].replace(/\t/g, "  ").length + 2;
        out.push(...quote(noteList(this.journal, list), indent));
      }
    }
    return out;
  }
}

/**
 * A report period as copied: `markdown` is the version on screen as displayed ("" when there
 * is none yet).
 */
export function periodMarkdown({
  markdown,
  journal,
  projects,
  filter,
  labels,
}: {
  markdown: string;
  journal: JournalNodes;
  projects: ProjectRef[];
  filter: SourceFilter;
  labels: CopyLabels;
}): string {
  if (filter === "agent") return spaceHeadings(markdown);
  const combined = filter === "both";
  const w = new Writer(journal, labels, combined);
  const lines = markdown === "" ? [] : markdown.split("\n");
  const layout = periodLayout(journal, projects, lines);
  const { sections, notesOf, annotationsAt } = layout;
  const points = (line: number) => annotationsAt("point", line);
  const blocks: Block[] = [];

  let start = 0;
  if (lines.length > 0 && /^#\s/.test(lines[0])) {
    blocks.push([lines[0]]);
    start = 1;
  }
  const dayNotes = notesOf(null);
  if (dayNotes.length > 0) blocks.push([`## ${labels.dayNotes}`], w.notes(dayNotes));
  if (combined) {
    blocks.push(w.agentLines(lines, start, sections[0]?.headingLine ?? lines.length, points));
  }

  const footer = (project: ProjectRef | null, headingLine: number | null) => {
    if (headingLine !== null) blocks.push(...w.orphans(annotationsAt("section", headingLine)));
    if (project) {
      blocks.push(...w.orphans(layout.detachedOf(project.id)));
      blocks.push(w.notes(notesOf(project.id, "bottom")));
    }
  };

  sections.forEach((s, idx) => {
    const end = sections[idx + 1]?.headingLine ?? lines.length;
    if (!combined && !layout.sectionHasNotes(s)) return;
    blocks.push([combined ? lines[s.headingLine].trim() : `## ${s.heading}`]);
    if (s.project) blocks.push(w.notes(notesOf(s.project.id, "top")));
    if (combined) {
      blocks.push(w.agentLines(lines, s.headingLine + 1, end, points));
    } else {
      for (const i of layout.annotatedPoints(s)) {
        blocks.push(w.withContext(pointText(lines[i]), points(i)));
      }
    }
    footer(s.project, s.headingLine);
  });

  for (const p of layout.notesOnlyProjects) {
    blocks.push([`## ${p.name}`], w.notes(notesOf(p.id, "top")));
    footer(p, null);
  }
  if (layout.looseDetached.length > 0) {
    blocks.push([`## ${labels.otherAnnotations}`], ...w.orphans(layout.looseDetached));
  }
  return join(blocks);
}

export interface RollupEntryText {
  period: string;
  label: string;
  /** The project's section of the period, without its heading ("" when there is none). */
  markdown: string;
}

/** A project page as copied, its period entries in the order shown. */
export function rollupMarkdown({
  title,
  entries,
  periodKind,
  journal,
  filter,
  labels,
}: {
  /** The `# ` line. */
  title: string;
  entries: RollupEntryText[];
  /** The journal kind of the entries' periods (`day`, `week`, `month`). */
  periodKind: string;
  journal: JournalNodes;
  filter: SourceFilter;
  labels: CopyLabels;
}): string {
  const showAgent = filter !== "human";
  const combined = filter === "both";
  const w = new Writer(journal, labels, combined);
  const blocks: Block[] = [[title]];

  const page = filter === "agent" ? [] : pageNotesOf(journal);
  if (page.length > 0) blocks.push([`## ${labels.pageNotes}`], w.notes(page));

  for (const e of entries) {
    const text = e.markdown.trim();
    const hasAgent = showAgent && text !== "";
    const layout = entryLayout(journal, periodKind, e.period);
    if (filter === "agent") {
      if (hasAgent) blocks.push([`## ${e.label}`], [text]);
      continue;
    }
    if (!hasAgent && !layout.hasNotes) continue;
    blocks.push([`## ${e.label}`], w.notes(layout.top));
    if (hasAgent) {
      const lines = e.markdown.split("\n");
      blocks.push(w.agentLines(lines, 0, lines.length, layout.pointAnnotations));
    } else if (!combined) {
      for (const n of layout.onPoints) blocks.push(w.withContext(n.quote ?? "", [n]));
    }
    blocks.push(...w.orphans(layout.elsewhere), w.notes(layout.bottom));
  }
  return join(blocks);
}
