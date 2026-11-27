import { useEffect, useRef, useState } from "react";
import { AgentBadge } from "../AgentBadge";
import { CopyButton } from "../CopyButton";
import type { ProjectOption, SliceDetail, TurnDetail } from "../../types";
import { SplitHandle } from "../Resizable";
import { useT } from "../../i18n";
import type { Messages } from "../../locales/en";
import { modelName } from "../../modelName";
import { assignableProjects } from "../../projectOptions";

interface SliceDrawerProps {
  width: number;
  onResizeStart: (e: React.MouseEvent) => void;
  detail: SliceDetail | null;
  loading: boolean;
  projects: ProjectOption[];
  highlightTurn: number | null;
  digesting: boolean;
  /** The model alias reports are generated with now (null when none is configured). */
  currentModel: string | null;
  onClose: () => void;
  onJumpToTurn: (turnIndex: number) => void;
  onSaveNote: (sessionId: number, note: string | null) => Promise<void>;
  onSetSliceIgnoreState: (
    sliceId: number,
    state: "inherit" | "ignored" | "included",
  ) => Promise<void>;
  onAssignSliceOnly: (
    sliceId: number,
    projectId: number | null,
  ) => Promise<void>;
  onDigestSlice: (sliceId: number) => Promise<void>;
}

/** One turn as Markdown, as the card shows it: the user's prompt and the agent's final reply. */
function turnMarkdown(
  turn: TurnDetail,
  labels: { user: string; result: string; noPrompt: string },
): string {
  const parts = [`**${labels.user}** ${turn.userPrompt || labels.noPrompt}`];
  if (turn.conclusion) parts.push(`**${labels.result}**\n\n${turn.conclusion}`);
  return parts.join("\n\n") + "\n";
}

function TurnItemCard({
  turn,
  highlighted,
  turnRef,
}: {
  turn: TurnDetail;
  highlighted: boolean;
  turnRef: (el: HTMLDivElement | null) => void;
}) {
  const td = useT().drawer;
  const [expanded, setExpanded] = useState(false);
  const extraMessages = Math.max(0, turn.messages.length - 1);

  return (
    <div
      ref={turnRef}
      className={`turn-card ${highlighted ? "highlighted" : ""}`}
    >
      <div className="turn-header">
        <span className="turn-index">T{turn.turnIndex}</span>
        <span className="turn-header-right">
          <CopyButton text={() => turnMarkdown(turn, td)} />
          <span className="muted-line">{turn.time}</span>
        </span>
      </div>
      <div className="turn-prompt">
        <strong>{td.user}</strong>{" "}
        {turn.userPrompt || td.noPrompt}
      </div>
      {turn.conclusion && (
        <div className="turn-conclusion">
          <strong>{td.result}</strong> {turn.conclusion}
        </div>
      )}
      {(turn.tools.length > 0 || extraMessages > 0) && (
        <div className="turn-extra">
          <button
            type="button"
            className="btn btn-ghost btn-xs"
            onClick={() => setExpanded((v) => !v)}
          >
            {td.details(expanded, turn.tools.length, extraMessages)}
          </button>
          {expanded && (
            <div className="turn-details">
              {extraMessages > 0 &&
                turn.messages.slice(0, -1).map((m, idx) => (
                  <div key={idx} className="turn-submsg">
                    {m}
                  </div>
                ))}
              {turn.tools.map((tool, idx) => (
                <div key={idx} className="tool-line">
                  <code>{tool.toolName}</code>
                  {tool.target && (
                    <span className="tool-target">{tool.target}</span>
                  )}
                  {(tool.added !== null || tool.removed !== null) && (
                    <span className="tool-diff">
                      (+{tool.added ?? 0} -{tool.removed ?? 0})
                    </span>
                  )}
                  {tool.isError && (
                    <span className="badge badge-err">{td.error}</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Which model wrote the digest, and whether generating now would digest the slice again. */
function DigestModel({
  detail,
  currentModel,
  texts,
}: {
  detail: SliceDetail;
  currentModel: string | null;
  texts: Messages["drawer"];
}) {
  if (detail.digest?.kind === "raw") {
    return <div className="muted-line digest-model">{texts.digestNoModel}</div>;
  }
  const m = detail.digestModel;
  // Unknown: the digest may well be the current model's, so promise nothing.
  const other = currentModel !== null && m !== null && !m.aliases.includes(currentModel);
  return (
    <div className="muted-line digest-model">
      {m ? texts.digestModel(modelName(m.engine, m.aliases)) : texts.digestModelUnknown}
      {other && <> · {texts.digestOtherModel(currentModel)}</>}
    </div>
  );
}

export function SliceDrawer({
  width,
  onResizeStart,
  detail,
  loading,
  projects,
  highlightTurn,
  digesting,
  currentModel,
  onClose,
  onJumpToTurn,
  onSaveNote,
  onSetSliceIgnoreState,
  onAssignSliceOnly,
  onDigestSlice,
}: SliceDrawerProps) {
  const tr = useT();
  const td = tr.drawer;
  const [noteDraft, setNoteDraft] = useState("");
  const turnRefs = useRef<Map<number, HTMLDivElement | null>>(new Map());

  useEffect(() => {
    setNoteDraft(detail?.slice.userNote ?? "");
  }, [detail?.slice.id, detail?.slice.userNote]);

  useEffect(() => {
    if (highlightTurn !== null && detail) {
      const el = turnRefs.current.get(highlightTurn);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }
  }, [highlightTurn, detail]);

  const open = detail !== null || loading;
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside
        className="drawer"
        style={{ width: `${width}px` }}
        onClick={(e) => e.stopPropagation()}
      >
        <SplitHandle onMouseDown={onResizeStart} label={td.resize} />
        <div className="drawer-inner">
          <div className="drawer-header">
            <div>
              <h3>
                {detail
                  ? `S${detail.slice.id} · ${detail.slice.title}`
                  : td.loading}
              </h3>
              {detail && (
                <div className="muted-line drawer-meta-line">
                  <AgentBadge
                    agent={detail.slice.agent}
                    name={detail.slice.agentName}
                  />
                  {detail.slice.sliceDate} · {detail.slice.startTime}-
                  {detail.slice.endTime} · {detail.slice.shortId} ·{" "}
                  {td.chars(detail.dehydratedChars)}
                </div>
              )}
            </div>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={onClose}
            >
              {tr.common.close}
            </button>
          </div>

          {loading || !detail ? (
            <div className="empty-state">{td.loadingDetails}</div>
          ) : (
            <div className="drawer-body">
              <div className="drawer-meta-grid">
                {detail.detectedCwd && (
                  <div>
                    <span className="meta-k">{td.directory}</span>{" "}
                    <code>{detail.detectedCwd}</code>
                  </div>
                )}
                {detail.gitBranch && (
                  <div>
                    <span className="meta-k">{td.branch}</span>{" "}
                    <code>{detail.gitBranch}</code>
                  </div>
                )}
                {detail.gitRemote && (
                  <div>
                    <span className="meta-k">{td.remote}</span>{" "}
                    <code>{detail.gitRemote}</code>
                  </div>
                )}
                <div className="drawer-controls-row">
                  <label>
                    {td.projectOverride}
                    <select
                      value={
                        detail.slice.projectOverrideId !== null
                          ? String(detail.slice.projectOverrideId)
                          : ""
                      }
                      onChange={(e) =>
                        void onAssignSliceOnly(
                          detail.slice.id,
                          e.target.value ? Number(e.target.value) : null,
                        )
                      }
                    >
                      <option value="">{td.followSession}</option>
                      {assignableProjects(projects, detail.slice.projectOverrideId).map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    {td.ignoreState}
                    <select
                      value={detail.slice.ignoreState}
                      onChange={(e) =>
                        void onSetSliceIgnoreState(
                          detail.slice.id,
                          e.target.value as "inherit" | "ignored" | "included",
                        )
                      }
                    >
                      <option value="inherit">{td.inheritSession}</option>
                      <option value="included">{td.include}</option>
                      <option value="ignored">{td.ignore}</option>
                    </select>
                  </label>
                </div>
              </div>

              <div className="note-editor">
                <input
                  type="text"
                  placeholder={td.notePlaceholder}
                  value={noteDraft}
                  onChange={(e) => setNoteDraft(e.target.value)}
                />
                <button
                  type="button"
                  className="btn btn-secondary btn-sm"
                  onClick={() =>
                    void onSaveNote(detail.slice.sessionId, noteDraft || null)
                  }
                >
                  {td.saveNote}
                </button>
              </div>

              {detail.filesModified.length > 0 && (
                <details className="files-collapse">
                  <summary>
                    {td.modifiedFiles(detail.filesModified.length)}
                  </summary>
                  <ul className="files-list">
                    {detail.filesModified.map((f) => (
                      <li key={f}>
                        <code>{f}</code>
                      </li>
                    ))}
                  </ul>
                </details>
              )}

              <div className="digest-box">
                <div className="digest-box-header">
                  <h4>{td.digest}</h4>
                  <button
                    type="button"
                    className="btn btn-secondary btn-xs"
                    disabled={digesting}
                    onClick={() => void onDigestSlice(detail.slice.id)}
                  >
                    {digesting
                      ? td.summarizing
                      : detail.digest
                        ? td.refreshDigest
                        : td.generateDigest}
                  </button>
                </div>
                {detail.digest && (
                  <DigestModel
                    detail={detail}
                    currentModel={currentModel}
                    texts={td}
                  />
                )}
                {!detail.digest ? (
                  <div className="muted-line">{td.noDigest}</div>
                ) : detail.digest.kind === "raw" ? (
                  <pre className="raw-digest">{detail.digest.text}</pre>
                ) : (
                  <div className="digest-summary">
                    <div>
                      <strong>{td.goal}</strong> {detail.digest.goal}
                    </div>
                    {detail.digest.key_changes.length > 0 && (
                      <div>
                        <strong>{td.keyChanges}</strong>
                        <ul>
                          {detail.digest.key_changes.map((c, i) => (
                            <li key={i}>
                              {c.text}{" "}
                              {c.turns.map((t) => (
                                <button
                                  key={t}
                                  type="button"
                                  className="anchor-chip"
                                  onClick={() => onJumpToTurn(t)}
                                >
                                  T{t}
                                </button>
                              ))}
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {detail.digest.decisions_and_pitfalls.length > 0 && (
                      <div>
                        <strong>{td.decisions}</strong>
                        <ul>
                          {detail.digest.decisions_and_pitfalls.map((c, i) => (
                            <li key={i}>
                              {c.text}{" "}
                              {c.turns.map((t) => (
                                <button
                                  key={t}
                                  type="button"
                                  className="anchor-chip"
                                  onClick={() => onJumpToTurn(t)}
                                >
                                  T{t}
                                </button>
                              ))}
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}
                    <div className="muted-line">
                      {td.outcome(detail.digest.outcome)}
                      {detail.digest.open_items.length > 0 &&
                        td.open(detail.digest.open_items.join("; "))}
                    </div>
                  </div>
                )}
              </div>

              <div className="turns-section">
                <h4>{td.turns(detail.turns.length)}</h4>
                {detail.turns.map((turn) => (
                  <TurnItemCard
                    key={turn.turnIndex}
                    turn={turn}
                    highlighted={highlightTurn === turn.turnIndex}
                    turnRef={(el) => {
                      turnRefs.current.set(turn.turnIndex, el);
                    }}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      </aside>
    </div>
  );
}
