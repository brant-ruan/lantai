import { useState } from "react";
import type { ProjectCandidate, ProjectsOverview } from "../../types";
import { InfoTip } from "../InfoTip";
import { useT } from "../../i18n";

interface ProjectsViewProps {
  mode: "projects" | "rules";
  overview: ProjectsOverview | null;
  onCreateProject: (params: {
    name: string;
    description: string | null;
    paths: string[];
    remotes: string[];
    autoApply: boolean;
  }) => Promise<void>;
  onRenameProject: (id: number, newName: string) => Promise<void>;
  onDescribeProject: (id: number, description: string | null) => Promise<void>;
  onDeleteProject: (id: number) => void;
  onSetProjectArchived: (id: number, archived: boolean) => void;
  onAddRule: (params: {
    projectId: number;
    ruleType: string;
    targetField: string | null;
    pattern: string;
    autoApply: boolean;
  }) => Promise<void>;
  onDeleteRule: (id: number) => Promise<void>;
  onSetRuleAuto: (id: number, autoApply: boolean) => Promise<void>;
}

export function ProjectsView({
  mode,
  overview,
  onCreateProject,
  onRenameProject,
  onDescribeProject,
  onDeleteProject,
  onSetProjectArchived,
  onAddRule,
  onDeleteRule,
  onSetRuleAuto,
}: ProjectsViewProps) {
  const tr = useT();
  const t = tr.projectsSettings;
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [pathRule, setPathRule] = useState("");
  const [remoteRule, setRemoteRule] = useState("");
  const [autoApply, setAutoApply] = useState(false);

  const [editingId, setEditingId] = useState<number | null>(null);
  const [editName, setEditName] = useState("");
  const [editDesc, setEditDesc] = useState("");

  const [ruleProjId, setRuleProjId] = useState<number | "">("");
  const [ruleType, setRuleType] = useState("path_prefix");
  const [ruleField, setRuleField] = useState("any");
  const [rulePattern, setRulePattern] = useState("");
  const [ruleAuto, setRuleAuto] = useState(false);

  const projects = overview?.projects ?? [];
  const rules = overview?.rules ?? [];
  const archivedIds = new Set(projects.filter((p) => p.archived).map((p) => p.id));
  const assignable = projects.filter((p) => !p.archived);
  const candidates = overview?.candidates ?? [];

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    await onCreateProject({
      name: name.trim(),
      description: desc.trim() || null,
      paths: pathRule.trim() ? [pathRule.trim()] : [],
      remotes: remoteRule.trim() ? [remoteRule.trim()] : [],
      autoApply,
    });
    setName("");
    setDesc("");
    setPathRule("");
    setRemoteRule("");
    setAutoApply(false);
  };

  const prefillCandidate = (c: ProjectCandidate) => {
    const base = c.root.split("/").filter(Boolean).pop() ?? "Project";
    setName(base);
    setPathRule(c.root);
    setRemoteRule(c.gitRemote ?? "");
  };

  const handleAddRule = async (e: React.FormEvent) => {
    e.preventDefault();
    const pid = Number(ruleProjId || assignable[0]?.id);
    if (!pid || !rulePattern.trim()) return;
    await onAddRule({
      projectId: pid,
      ruleType,
      targetField: ruleType === "regex" ? ruleField : null,
      pattern: rulePattern.trim(),
      autoApply: ruleAuto,
    });
    setRulePattern("");
  };

  if (mode === "rules") {
    return (
      <div className="settings-stack">
        <form
          className="settings-card stack"
          onSubmit={(e) => void handleAddRule(e)}
        >
          <div className="settings-card-head">
            <h3 className="title-with-tip">
              {t.addRule}
              <InfoTip>{t.addRuleTip}</InfoTip>
            </h3>
          </div>

          <div className="rule-builder-row">
            <select
              value={ruleProjId || assignable[0]?.id || ""}
              onChange={(e) => setRuleProjId(Number(e.target.value))}
            >
              {assignable.length === 0 ? (
                <option value="">{t.noProjects}</option>
              ) : (
                assignable.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))
              )}
            </select>
            <select
              value={ruleType}
              onChange={(e) => setRuleType(e.target.value)}
            >
              {["path_prefix", "git_remote", "regex"].map((k) => (
                <option key={k} value={k}>
                  {t.ruleTypes[k]}
                </option>
              ))}
            </select>
            {ruleType === "regex" && (
              <select
                value={ruleField}
                onChange={(e) => setRuleField(e.target.value)}
              >
                {["any", "title", "first_prompt", "cwd"].map((k) => (
                  <option key={k} value={k}>
                    {t.fields[k]}
                  </option>
                ))}
              </select>
            )}
            <input
              type="text"
              className="flex-1"
              placeholder={
                ruleType === "path_prefix"
                  ? "/abs/path/to/repo"
                  : ruleType === "git_remote"
                    ? "github.com/owner/repo"
                    : t.regexPlaceholder
              }
              value={rulePattern}
              onChange={(e) => setRulePattern(e.target.value)}
              required
            />
          </div>

          <div className="settings-card-foot">
            <label className="toggle-row">
              <input
                type="checkbox"
                checked={ruleAuto}
                onChange={(e) => setRuleAuto(e.target.checked)}
              />
              <span>{t.autoOnMatch}</span>
            </label>
            <button
              type="submit"
              className="btn btn-primary btn-sm"
              disabled={projects.length === 0}
            >
              {t.addRule}
            </button>
          </div>
        </form>

        <div className="settings-card">
          <div className="settings-card-head">
            <h3>{t.activeRules(rules.length)}</h3>
          </div>
          {rules.length === 0 ? (
            <div className="empty-state">{t.noRules}</div>
          ) : (
            <div className="settings-list">
              {rules.map((r) => (
                <div key={r.id} className="settings-item-row">
                  <div className="settings-item-main">
                    <div className="slice-title-line">
                      <strong>{r.projectName}</strong>
                      <span className="badge">
                        {t.ruleTypes[r.ruleType] ?? r.ruleType}
                        {r.targetField ? `(${r.targetField})` : ""}
                      </span>
                      {archivedIds.has(r.projectId) && (
                        <span className="badge" title={tr.projectArchive.rulesPaused}>
                          {tr.projectArchive.archived}
                        </span>
                      )}
                    </div>
                    <code className="rule-pattern-code">{r.pattern}</code>
                  </div>
                  <div className="settings-item-actions">
                    <label className="toggle-row">
                      <input
                        type="checkbox"
                        checked={r.autoApply}
                        onChange={(e) =>
                          void onSetRuleAuto(r.id, e.target.checked)
                        }
                      />
                      <span>{t.auto}</span>
                    </label>
                    <button
                      type="button"
                      className="btn btn-ghost-danger btn-xs"
                      onClick={() => void onDeleteRule(r.id)}
                    >
                      {tr.common.delete}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="settings-stack">
      <form className="settings-card stack" onSubmit={(e) => void handleCreate(e)}>
        <div className="settings-card-head">
          <h3>{t.createProject}</h3>
        </div>

        <div className="form-grid-2">
          <label className="form-field">
            <span>{t.name}</span>
            <input
              type="text"
              placeholder={t.namePlaceholder}
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
            />
          </label>
          <label className="form-field">
            <span className="title-with-tip">
              {t.description}
              <InfoTip>{t.descriptionTip}</InfoTip>
            </span>
            <input
              type="text"
              placeholder={t.optional}
              value={desc}
              onChange={(e) => setDesc(e.target.value)}
            />
          </label>
          <label className="form-field">
            <span>{t.dirPrefix}</span>
            <input
              type="text"
              placeholder="/abs/path/to/repo"
              value={pathRule}
              onChange={(e) => setPathRule(e.target.value)}
            />
          </label>
          <label className="form-field">
            <span>{t.gitRemote}</span>
            <input
              type="text"
              placeholder="github.com/owner/repo"
              value={remoteRule}
              onChange={(e) => setRemoteRule(e.target.value)}
            />
          </label>
        </div>

        <div className="settings-card-foot">
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={autoApply}
              onChange={(e) => setAutoApply(e.target.checked)}
            />
            <span>{t.autoStarter}</span>
          </label>
          <button type="submit" className="btn btn-primary btn-sm">
            {t.createProject}
          </button>
        </div>
      </form>

      <div className="settings-card">
        <div className="settings-card-head">
          <h3>{t.configured(projects.length)}</h3>
        </div>
        {projects.length === 0 ? (
          <div className="empty-state">{t.noneCreated}</div>
        ) : (
          <div className="settings-list">
            {projects.map((p) => (
              <div key={p.id} className="settings-item-row">
                {editingId === p.id ? (
                  <div className="stack flex-1">
                    <div className="form-grid-2">
                      <input
                        type="text"
                        value={editName}
                        placeholder={t.projectName}
                        onChange={(e) => setEditName(e.target.value)}
                      />
                      <input
                        type="text"
                        placeholder={t.descriptionPlaceholder}
                        value={editDesc}
                        onChange={(e) => setEditDesc(e.target.value)}
                      />
                    </div>
                    <div className="toolbar-actions">
                      <button
                        type="button"
                        className="btn btn-primary btn-xs"
                        onClick={async () => {
                          if (editName.trim() && editName.trim() !== p.name) {
                            await onRenameProject(p.id, editName.trim());
                          }
                          await onDescribeProject(p.id, editDesc.trim() || null);
                          setEditingId(null);
                        }}
                      >
                        {tr.common.save}
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => setEditingId(null)}
                      >
                        {tr.common.cancel}
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="settings-item-main">
                      <div className="slice-title-line">
                        <strong>{p.name}</strong>
                        {p.archived && (
                          <span className="badge">{tr.projectArchive.archived}</span>
                        )}
                        <span className="muted-line">
                          {t.counts(p.slices, p.suggested, p.rules)}
                        </span>
                      </div>
                      {p.description && (
                        <div className="muted-line">{p.description}</div>
                      )}
                    </div>
                    <div className="settings-item-actions">
                      <button
                        type="button"
                        className="btn btn-secondary btn-xs"
                        onClick={() => {
                          setEditingId(p.id);
                          setEditName(p.name);
                          setEditDesc(p.description ?? "");
                        }}
                      >
                        {t.edit}
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={() => onSetProjectArchived(p.id, !p.archived)}
                      >
                        {p.archived ? tr.projectArchive.restore : tr.projectArchive.archive}
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost-danger btn-xs"
                        onClick={() => onDeleteProject(p.id)}
                      >
                        {tr.common.delete}
                      </button>
                    </div>
                  </>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {candidates.length > 0 && (
        <div className="settings-card">
          <div className="settings-card-head">
            <h3>{t.clusters(candidates.length)}</h3>
          </div>
          <div className="settings-list">
            {candidates.map((c) => (
              <div key={c.root} className="settings-item-row">
                <div className="settings-item-main">
                  <div className="slice-title-line">
                    <code>{c.root}</code>
                    <span className="badge">{t.sessions(c.sessions)}</span>
                  </div>
                  {c.gitRemote && (
                    <div className="muted-line">{t.remote(c.gitRemote)}</div>
                  )}
                  {c.sampleTitles.length > 0 && (
                    <div className="muted-line">
                      {c.sampleTitles.join(" · ")}
                    </div>
                  )}
                </div>
                <div className="settings-item-actions">
                  <button
                    type="button"
                    className="btn btn-secondary btn-xs"
                    onClick={() => prefillCandidate(c)}
                  >
                    {t.useTemplate}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
