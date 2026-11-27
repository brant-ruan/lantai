import { useEffect, useState } from "react";
import { DENSITY_OPTIONS, THEMES } from "../../themes";
import type {
  DensityId,
  Diagnostics,
  ModelSettings,
  ProjectsOverview,
  PromptItem,
  AgentSource,
  SettingsTab,
  ThemeId,
} from "../../types";
import { AboutView } from "./AboutView";
import { AgentsView } from "./AgentsView";
import { AppIconPicker } from "./AppIconPicker";
import { BackupSetting } from "./BackupSetting";
import { CliToolSetting } from "./CliToolSetting";
import { UpdateSetting } from "./UpdateSetting";
import type { UpdateCheckApi } from "../../hooks/useUpdateCheck";
import { DayBoundarySetting } from "./DayBoundarySetting";
import { HealthView } from "./HealthView";
import { InfoTip } from "../InfoTip";
import { ModelsView } from "./models/ModelsView";
import { ProjectsView } from "./ProjectsView";
import { PromptsView, promptsForReportLanguage } from "./PromptsView";
import { useLanguage } from "../../i18n";
import { LanguageSettings } from "./LanguageSettings";
import {
  REPORT_WIDTH_DEFAULT,
  REPORT_WIDTH_MAX,
  REPORT_WIDTH_MIN,
  REPORT_WIDTH_STEP,
  resetReportWidth,
  setReportWidth,
  useReportWidth,
} from "../../hooks/useReportWidth";

interface SettingsModalProps {
  open: boolean;
  onClose: () => void;
  /** Selected category; owned by the caller so menu items can open a specific one. */
  tab: SettingsTab;
  onChangeTab: (tab: SettingsTab) => void;
  theme: ThemeId;
  onChangeTheme: (theme: ThemeId) => void;
  density: DensityId;
  onChangeDensity: (density: DensityId) => void;
  layoutSizes: { drawer: number };
  onResetLayout: () => void;
  prompts: PromptItem[];
  onCreatePrompt: (kind: string, name: string, body: string) => Promise<void>;
  onUpdatePrompt: (id: number, body: string) => Promise<void>;
  onRenamePrompt: (id: number, newName: string) => Promise<void>;
  onDeletePrompt: (id: number) => Promise<void>;
  onSetDefaultPrompt: (id: number) => Promise<void>;
  projectsOverview: ProjectsOverview | null;
  onCreateProject: (params: {
    name: string;
    description: string | null;
    paths: string[];
    remotes: string[];
    autoApply: boolean;
  }) => Promise<void>;
  onRenameProject: (id: number, newName: string) => Promise<void>;
  onDescribeProject: (id: number, description: string | null) => Promise<void>;
  /** Asks for confirmation before deleting. */
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
  health: Diagnostics | null;
  onRefreshHealth: () => Promise<void>;
  onModelsChanged: (settings: ModelSettings) => void;
  agentSources: AgentSource[];
  onUpdateAgent: (agent: string, enabled: boolean, preDigest: boolean) => void;
  idleStatus: string | null;
  /** Called after the day boundary changed and sessions were re-filed. */
  onDayBoundaryChanged: () => Promise<void>;
  updates: UpdateCheckApi;
}

/** Categories in sidebar order. */
const TABS: SettingsTab[] = [
  "appearance",
  "language",
  "prompts",
  "projects",
  "rules",
  "models",
  "agents",
  "system",
  "about",
];

function CategoryIcon({ tab }: { tab: SettingsTab }) {
  const common = {
    width: 15,
    height: 15,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (tab) {
    case "appearance":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 3v18M12 8l6-3M12 13l7-2" />
        </svg>
      );
    case "prompts":
      return (
        <svg {...common}>
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
          <line x1="8" y1="13" x2="16" y2="13" />
          <line x1="8" y1="17" x2="13" y2="17" />
        </svg>
      );
    case "language":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
        </svg>
      );
    case "projects":
      return (
        <svg {...common}>
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
      );
    case "rules":
      return (
        <svg {...common}>
          <line x1="4" y1="7" x2="20" y2="7" />
          <circle cx="9" cy="7" r="2" />
          <line x1="4" y1="17" x2="20" y2="17" />
          <circle cx="15" cy="17" r="2" />
        </svg>
      );
    case "models":
      return (
        <svg {...common}>
          <path d="M12 2l8 4.5v9L12 20l-8-4.5v-9z" />
          <path d="M12 11l8-4.5M12 11v9M12 11L4 6.5" />
        </svg>
      );
    case "agents":
      return (
        <svg {...common}>
          {/* Robot head: antenna, face, eyes and side ears. */}
          <path d="M12 3v3" />
          <circle cx="12" cy="2.6" r="0.6" fill="currentColor" />
          <rect x="4.5" y="6" width="15" height="12" rx="3" />
          <path d="M2.5 11v3M21.5 11v3" />
          <circle cx="9.3" cy="11.5" r="1" fill="currentColor" />
          <circle cx="14.7" cy="11.5" r="1" fill="currentColor" />
          <path d="M9.5 15h5" />
        </svg>
      );
    case "about":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="9" />
          <line x1="12" y1="11" x2="12" y2="16.5" />
          <circle cx="12" cy="7.6" r="0.6" fill="currentColor" />
        </svg>
      );
    case "system":
      return (
        <svg {...common}>
          <rect x="4" y="4" width="16" height="16" rx="2" />
          <rect x="9" y="9" width="6" height="6" />
          <line x1="9" y1="1" x2="9" y2="4" />
          <line x1="15" y1="1" x2="15" y2="4" />
          <line x1="9" y1="20" x2="9" y2="23" />
          <line x1="15" y1="20" x2="15" y2="23" />
          <line x1="20" y1="9" x2="23" y2="9" />
          <line x1="20" y1="14" x2="23" y2="14" />
          <line x1="1" y1="9" x2="4" y2="9" />
          <line x1="1" y1="14" x2="4" y2="14" />
        </svg>
      );
  }
}

export function SettingsModal({
  open,
  onClose,
  tab,
  onChangeTab,
  theme,
  onChangeTheme,
  density,
  onChangeDensity,
  layoutSizes,
  onResetLayout,
  prompts,
  onCreatePrompt,
  onUpdatePrompt,
  onRenamePrompt,
  onDeletePrompt,
  onSetDefaultPrompt,
  projectsOverview,
  onCreateProject,
  onRenameProject,
  onDescribeProject,
  onDeleteProject,
  onSetProjectArchived,
  onAddRule,
  onDeleteRule,
  onSetRuleAuto,
  health,
  onRefreshHealth,
  onModelsChanged,
  agentSources,
  onUpdateAgent,
  idleStatus,
  onDayBoundaryChanged,
  updates,
}: SettingsModalProps) {
  const { t: tr, reportLanguage, reportLanguages } = useLanguage();
  const t = tr.settings;
  const [resetNotice, setResetNotice] = useState(false);
  const reportWidth = useReportWidth();

  const handleClose = () => {
    setResetNotice(false);
    onClose();
  };

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setResetNotice(false);
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  const lightThemes = THEMES.filter((th) => th.mode === "light");
  const darkThemes = THEMES.filter((th) => th.mode === "dark");
  const activeMeta = t.tabs[tab];

  const badgeCount = (id: SettingsTab): number | null => {
    if (id === "prompts")
      return promptsForReportLanguage(prompts, reportLanguage, reportLanguages).length;
    if (id === "projects") return projectsOverview?.projects.length ?? null;
    if (id === "rules") return projectsOverview?.rules.length ?? null;
    if (id === "agents") return agentSources.filter((a) => a.enabled).length;
    return null;
  };

  return (
    <div className="modal-backdrop" onClick={handleClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <aside className="settings-sidebar">
          <div className="settings-sidebar-title">{t.title}</div>
          <nav className="settings-nav" aria-label={t.categories}>
            {TABS.map((id) => {
              const count = badgeCount(id);
              return (
                <button
                  key={id}
                  type="button"
                  className={`settings-nav-item ${tab === id ? "active" : ""}`}
                  onClick={() => onChangeTab(id)}
                >
                  <CategoryIcon tab={id} />
                  <span className="settings-nav-label">{t.tabs[id].label}</span>
                  {count !== null && (
                    <span className="settings-nav-count">{count}</span>
                  )}
                </button>
              );
            })}
          </nav>
        </aside>

        <div className="settings-main">
          <header className="settings-main-header">
            <div>
              <h2>{activeMeta.title}</h2>
            </div>
            <button
              type="button"
              className="btn btn-ghost btn-icon-sq"
              onClick={handleClose}
              title={t.close}
              aria-label={t.close}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </header>

          <div
            className={`settings-main-body ${tab === "prompts" ? "flush" : ""}`}
          >
            {tab === "appearance" && (
              <div className="settings-stack">
                <div className="settings-card">
                  <div className="settings-card-head">
                    <h3>{t.colorTheme}</h3>
                  </div>

                  <div className="theme-group-label">{t.light}</div>
                  <div className="theme-grid">
                    {lightThemes.map((th) => (
                      <button
                        key={th.id}
                        type="button"
                        className={`theme-card ${theme === th.id ? "active" : ""}`}
                        onClick={() => onChangeTheme(th.id)}
                      >
                        <div className="theme-swatches">
                          {th.swatches.map((hex, idx) => (
                            <span
                              key={idx}
                              className="swatch-dot"
                              style={{ backgroundColor: hex }}
                            />
                          ))}
                        </div>
                        <div className="theme-name">{th.name}</div>
                        <div className="theme-sub">{t.themeSubtitles[th.id]}</div>
                      </button>
                    ))}
                  </div>

                  <div className="theme-group-label">{t.dark}</div>
                  <div className="theme-grid">
                    {darkThemes.map((th) => (
                      <button
                        key={th.id}
                        type="button"
                        className={`theme-card ${theme === th.id ? "active" : ""}`}
                        onClick={() => onChangeTheme(th.id)}
                      >
                        <div className="theme-swatches">
                          {th.swatches.map((hex, idx) => (
                            <span
                              key={idx}
                              className="swatch-dot"
                              style={{ backgroundColor: hex }}
                            />
                          ))}
                        </div>
                        <div className="theme-name">{th.name}</div>
                        <div className="theme-sub">{t.themeSubtitles[th.id]}</div>
                      </button>
                    ))}
                  </div>
                </div>

                <AppIconPicker />

                <div className="settings-card">
                  <div className="settings-card-head">
                    <h3>{t.spacingLayout}</h3>
                  </div>

                  <div className="settings-row">
                    <div className="settings-row-title">{t.density}</div>
                    <div className="pill-group">
                      {DENSITY_OPTIONS.map((id) => (
                        <button
                          key={id}
                          type="button"
                          className={`pill ${density === id ? "active" : ""}`}
                          onClick={() => {
                            setResetNotice(false);
                            onChangeDensity(id);
                          }}
                          title={t.densities[id].hint}
                        >
                          {t.densities[id].label}
                        </button>
                      ))}
                    </div>
                  </div>

                  <div className="settings-row">
                    <div>
                      <div className="settings-row-title">{t.paneProportions}</div>
                      <div className="muted-line">
                        {t.drawerWidth(Math.round(layoutSizes.drawer))}
                      </div>
                    </div>
                    <button
                      type="button"
                      className="btn btn-secondary btn-sm"
                      onClick={() => {
                        onResetLayout();
                        setResetNotice(true);
                      }}
                    >
                      {resetNotice ? t.resetDone : t.resetLayout}
                    </button>
                  </div>

                  <div className="settings-row">
                    <div>
                      <div className="settings-row-title">{t.reportWidth}</div>
                      <div className="muted-line">
                        {t.reportWidthHint(reportWidth, REPORT_WIDTH_DEFAULT)}
                      </div>
                    </div>
                    <div className="settings-range-row">
                      <input
                        type="range"
                        className="settings-range"
                        min={REPORT_WIDTH_MIN}
                        max={REPORT_WIDTH_MAX}
                        step={REPORT_WIDTH_STEP}
                        value={reportWidth}
                        aria-label={t.reportWidth}
                        onChange={(e) => setReportWidth(Number(e.target.value))}
                      />
                      <button
                        type="button"
                        className="btn btn-secondary btn-sm"
                        disabled={reportWidth === REPORT_WIDTH_DEFAULT}
                        onClick={resetReportWidth}
                      >
                        {t.resetReportWidth}
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {tab === "language" && <LanguageSettings />}

            {tab === "prompts" && (
              <PromptsView
                prompts={prompts}
                onCreate={onCreatePrompt}
                onUpdate={onUpdatePrompt}
                onRename={onRenamePrompt}
                onDelete={onDeletePrompt}
                onSetDefault={onSetDefaultPrompt}
              />
            )}

            {(tab === "projects" || tab === "rules") && (
              <ProjectsView
                mode={tab}
                overview={projectsOverview}
                onCreateProject={onCreateProject}
                onRenameProject={onRenameProject}
                onDescribeProject={onDescribeProject}
                onDeleteProject={onDeleteProject}
                onSetProjectArchived={onSetProjectArchived}
                onAddRule={onAddRule}
                onDeleteRule={onDeleteRule}
                onSetRuleAuto={onSetRuleAuto}
              />
            )}

            {tab === "models" && (
              <div className="settings-stack">
                <ModelsView onChanged={onModelsChanged} />
              </div>
            )}

            {tab === "agents" && (
              <div className="settings-stack">
                <AgentsView sources={agentSources} onUpdate={onUpdateAgent} />
              </div>
            )}

            {tab === "about" && <AboutView />}

            {tab === "system" && (
              <div className="settings-stack">
                <DayBoundarySetting onChanged={onDayBoundaryChanged} />

                <div className="settings-card">
                  <div className="settings-row clean">
                    <div>
                      <div className="settings-row-title title-with-tip">
                        {t.preDigest}
                        <InfoTip>{t.preDigestTip}</InfoTip>
                      </div>
                      {idleStatus && <div className="muted-line">{idleStatus}</div>}
                    </div>
                    <div className="predigest-agents">
                      {agentSources
                        .filter((a) => a.enabled)
                        .map((a) => (
                          <label key={a.agent} className="toggle-row">
                            <input
                              type="checkbox"
                              checked={a.preDigest}
                              onChange={(e) =>
                                onUpdateAgent(a.agent, true, e.target.checked)
                              }
                            />
                            <span>{a.name}</span>
                          </label>
                        ))}
                      {!agentSources.some((a) => a.enabled) && (
                        <span className="muted-line">{t.noAgentEnabled}</span>
                      )}
                    </div>
                  </div>
                </div>

                <BackupSetting />

                <UpdateSetting updates={updates} />

                <CliToolSetting />

                <HealthView health={health} onRefresh={onRefreshHealth} />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
