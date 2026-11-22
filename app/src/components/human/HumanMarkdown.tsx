// Renders the body of a human note: bold, italic, strikethrough, inline code and links;
// fenced code blocks; and links into the journal (source anchors like [#S12.T5], dates like
// 2026-11-20 and weeks like 2026-W47). Headings, tables, HTML and checkboxes stay plain text.
import React from "react";
import { AnchorChip, parseAnchorToken, type AnchorClickHandler } from "../../anchors";
import { api } from "../../api";

export interface JournalLinkHandlers {
  onSelectAnchor: AnchorClickHandler;
  /** Open a day (`2026-11-20`) or a week (`2026-W47`). */
  onOpenPeriod?: (key: string) => void;
}

const INLINE_RE =
  /(`[^`\n]+`|\*\*[^*\n]+\*\*|~~[^~\n]+~~|\*[^*\s][^*\n]*\*|\[[^\]\n]+\]\(https?:\/\/[^)\s]+\)|https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"]|\[#?S\d+(?:\.T\d+)?\]|\b\d{4}-\d{2}-\d{2}\b|\b\d{4}-W\d{2}\b)/g;
const LINK_RE = /^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/;

function WebLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a
      href={href}
      className="human-link"
      title={href}
      onClick={(e) => {
        e.preventDefault();
        void api.openNoteLink(href);
      }}
    >
      {children}
    </a>
  );
}

function renderInline(text: string, h: JournalLinkHandlers, key: string): React.ReactNode[] {
  return text.split(INLINE_RE).map((part, i) => {
    const k = `${key}-${i}`;
    if (!part) return null;
    if (i % 2 === 0) return <React.Fragment key={k}>{part}</React.Fragment>;
    if (part.startsWith("`")) return <code key={k}>{part.slice(1, -1)}</code>;
    if (part.startsWith("**")) return <strong key={k}>{renderInline(part.slice(2, -2), h, k)}</strong>;
    if (part.startsWith("~~")) return <del key={k}>{renderInline(part.slice(2, -2), h, k)}</del>;
    if (part.startsWith("*")) return <em key={k}>{renderInline(part.slice(1, -1), h, k)}</em>;
    const link = LINK_RE.exec(part);
    if (link) return <WebLink key={k} href={link[2]}>{link[1]}</WebLink>;
    if (part.startsWith("http")) return <WebLink key={k} href={part}>{part}</WebLink>;
    const anchors = parseAnchorToken(part);
    if (anchors) {
      return (
        <React.Fragment key={k}>
          {anchors.map((a, j) => (
            <AnchorChip key={j} item={a} onSelectAnchor={h.onSelectAnchor} />
          ))}
        </React.Fragment>
      );
    }
    if (h.onOpenPeriod) {
      const open = h.onOpenPeriod;
      return (
        <button key={k} type="button" className="human-date-link" onClick={() => open(part)}>
          {part}
        </button>
      );
    }
    return <React.Fragment key={k}>{part}</React.Fragment>;
  });
}

type Chunk = { type: "text"; lines: string[] } | { type: "code"; lang: string; lines: string[] };

function chunks(body: string): Chunk[] {
  const out: Chunk[] = [];
  let code: { lang: string; lines: string[] } | null = null;
  for (const line of body.split("\n")) {
    const fence = /^\s*```(.*)$/.exec(line);
    if (code) {
      if (fence) {
        out.push({ type: "code", ...code });
        code = null;
      } else {
        code.lines.push(line);
      }
      continue;
    }
    if (fence) {
      code = { lang: fence[1].trim(), lines: [] };
      continue;
    }
    const last = out[out.length - 1];
    if (last?.type === "text") last.lines.push(line);
    else out.push({ type: "text", lines: [line] });
  }
  // An unclosed fence still shows as code.
  if (code) out.push({ type: "code", ...code });
  return out;
}

/** Whether the caret at `pos` sits inside a fenced code block of `text`. */
export function insideCodeFence(text: string, pos: number): boolean {
  const before = text.slice(0, pos).split("\n");
  return before.filter((l) => /^\s*```/.test(l)).length % 2 === 1;
}

export function HumanMarkdown({ body, ...h }: { body: string } & JournalLinkHandlers) {
  return (
    <>
      {chunks(body).map((c, i) =>
        c.type === "code" ? (
          <pre key={i} className="human-code" data-lang={c.lang || undefined}>
            <code>{c.lines.join("\n")}</code>
          </pre>
        ) : (
          c.lines.map((line, j) => (
            <div key={`${i}-${j}`} className="human-line">
              {line ? renderInline(line, h, `${i}-${j}`) : " "}
            </div>
          ))
        ),
      )}
    </>
  );
}
