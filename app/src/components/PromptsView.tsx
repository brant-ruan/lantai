import { useEffect, useMemo, useState } from "react";
import { useLanguage } from "../i18n";
import type { PromptItem, ReportLanguage } from "../types";

/** Prompts offered for the report language: its own, the English built-ins it borrows when it
 * has no tuned prompts, and any default chosen from another language. */
export function promptsForReportLanguage(
  prompts: PromptItem[],
  reportLanguage: string,
  reportLanguages: ReportLanguage[],
): PromptItem[] {
  const borrowed = reportLanguages.find((l) => l.tag === reportLanguage)?.tuned === false;
  return prompts.filter(
    (p) => p.isDefault || p.language === reportLanguage || (borrowed && p.language === "en"),
  );
}

interface PromptsViewProps {
  prompts: PromptItem[];
  onCreate: (kind: string, name: string, body: string) => Promise<void>;
  onUpdate: (id: number, body: string) => Promise<void>;
  onRename: (id: number, newName: string) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
  onSetDefault: (id: number) => Promise<void>;
}

export function PromptsView({
  prompts: allPrompts,
  onCreate,
  onUpdate,
  onRename,
  onDelete,
  onSetDefault,
}: PromptsViewProps) {
  const { t: tr, reportLanguage, reportLanguages } = useLanguage();
  const t = tr.prompts;
  const kindName = (kind: string) => tr.reportKinds[kind] ?? kind;
  const languageName = (tag: string) =>
    reportLanguages.find((l) => l.tag === tag)?.nativeName ?? tag;
  // Prompts of the report language (and the built-ins it borrows), plus any default chosen
  // from another language; all of them with "Show all languages".
  const [showAll, setShowAll] = useState(false);
  const borrowed = reportLanguages.find((l) => l.tag === reportLanguage)?.tuned === false;
  const shown = useMemo(
    () =>
      showAll ? allPrompts : promptsForReportLanguage(allPrompts, reportLanguage, reportLanguages),
    [allPrompts, showAll, reportLanguage, reportLanguages],
  );
  const prompts = shown;
  const [selectedId, setSelectedId] = useState<number | null>(
    prompts[0]?.id ?? null,
  );
  const [bodyDraft, setBodyDraft] = useState("");
  const [renameDraft, setRenameDraft] = useState("");
  const [showNew, setShowNew] = useState(false);
  const [newName, setNewName] = useState("");
  const [newSourceId, setNewSourceId] = useState<number | "">("");
  const [showFull, setShowFull] = useState(false);
  const [savedNotice, setSavedNotice] = useState(false);

  const selected = prompts.find((p) => p.id === selectedId) ?? prompts[0] ?? null;

  useEffect(() => {
    if (selected) {
      setSelectedId(selected.id);
      setBodyDraft(selected.body);
      setRenameDraft(selected.name);
      setSavedNotice(false);
    }
  }, [selected?.id, selected?.body, selected?.name]);

  const handleDuplicate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim()) return;
    const base =
      prompts.find((p) => p.id === Number(newSourceId)) ?? selected ?? prompts[0];
    if (!base) return;
    await onCreate(base.kind, newName.trim(), base.body);
    setNewName("");
    setShowNew(false);
  };

  return (
    <div className="prompts-split">
      <div className="prompts-list-pane">
        <div className="prompts-list-head">
          <span className="section-label">{t.templates}</span>
          <button
            type="button"
            className="btn btn-secondary btn-xs"
            onClick={() => {
              setNewSourceId(selected?.id ?? "");
              setShowNew((v) => !v);
            }}
          >
            {tr.common.newItem}
          </button>
        </div>

        {showNew && (
          <form
            className="inline-card stack"
            onSubmit={(e) => void handleDuplicate(e)}
          >
            <input
              type="text"
              placeholder={t.newName}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              required
            />
            <select
              value={newSourceId}
              onChange={(e) => setNewSourceId(Number(e.target.value))}
            >
              {prompts.map((p) => (
                <option key={p.id} value={p.id}>
                  {t.copyOf(p.name, kindName(p.kind))}
                </option>
              ))}
            </select>
            <div className="toolbar-actions">
              <button type="submit" className="btn btn-primary btn-xs">
                {tr.common.create}
              </button>
              <button
                type="button"
                className="btn btn-ghost btn-xs"
                onClick={() => setShowNew(false)}
              >
                {tr.common.cancel}
              </button>
            </div>
          </form>
        )}

        <label className="toggle-row prompts-all-languages">
          <input
            type="checkbox"
            checked={showAll}
            onChange={(e) => setShowAll(e.target.checked)}
          />
          <span>{t.showAllLanguages}</span>
        </label>

        <div className="prompts-list">
          {prompts.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`prompt-nav-item ${selected?.id === p.id ? "active" : ""}`}
              onClick={() => setSelectedId(p.id)}
            >
              <div className="slice-title-line">
                <span className="prompt-nav-name">{p.name}</span>
                {p.language !== reportLanguage && (
                  <span className="badge">{languageName(p.language)}</span>
                )}
                {p.isDefault && <span className="badge badge-ok">{t.default}</span>}
              </div>
              <div className="muted-line">
                {kindName(p.kind)} · {p.isBuiltin ? t.builtIn : t.custom} ·{" "}
                {t.chars(p.body.length)}
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className="prompts-detail-pane">
        {!selected ? (
          <div className="empty-state">{t.selectHint}</div>
        ) : (
          <>
            <div className="prompts-detail-head">
              <div>
                <h3>{selected.name}</h3>
                <div className="muted-line">
                  {kindName(selected.kind)} · {languageName(selected.language)} ·{" "}
                  {selected.isBuiltin ? t.builtInTemplate : t.customPrompt}
                </div>
              </div>
              <div className="toolbar-actions">
                {!selected.isDefault && (
                  <button
                    type="button"
                    className="btn btn-secondary btn-xs"
                    onClick={() => void onSetDefault(selected.id)}
                  >
                    {t.setDefault}
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn-secondary btn-xs"
                  onClick={() => setShowFull((v) => !v)}
                >
                  {showFull ? t.editView : t.fullAssembled}
                </button>
                {!selected.isBuiltin && (
                  <button
                    type="button"
                    className="btn btn-ghost-danger btn-xs"
                    onClick={() => void onDelete(selected.id)}
                  >
                    {tr.common.delete}
                  </button>
                )}
              </div>
            </div>

            {selected.language !== reportLanguage &&
              !(borrowed && selected.language === "en") && (
                <div className="warning-list">
                  {t.otherLanguage(languageName(selected.language), languageName(reportLanguage))}
                </div>
              )}

            {!selected.isBuiltin && !showFull && (
              <div className="prompt-rename-row">
                <input
                  type="text"
                  className="flex-1"
                  value={renameDraft}
                  onChange={(e) => setRenameDraft(e.target.value)}
                />
                <button
                  type="button"
                  className="btn btn-secondary btn-sm"
                  disabled={
                    !renameDraft.trim() || renameDraft.trim() === selected.name
                  }
                  onClick={() => void onRename(selected.id, renameDraft.trim())}
                >
                  {tr.common.rename}
                </button>
              </div>
            )}

            {showFull ? (
              <pre className="compare-text full-height">
                {selected.assembledPreview}
              </pre>
            ) : (
              <div className="editor-wrap">
                <textarea
                  className="report-editor"
                  value={bodyDraft}
                  onChange={(e) => {
                    setSavedNotice(false);
                    setBodyDraft(e.target.value);
                  }}
                />
                <div className="editor-actions">
                  {savedNotice && <span className="muted-line">{tr.common.saved}</span>}
                  <button
                    type="button"
                    className="btn btn-primary btn-sm"
                    disabled={bodyDraft.trim() === selected.body.trim()}
                    onClick={async () => {
                      await onUpdate(selected.id, bodyDraft);
                      setSavedNotice(true);
                    }}
                  >
                    {t.savePrompt}
                  </button>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
