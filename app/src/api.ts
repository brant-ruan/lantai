import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BackupInfo,
  CliToolStatus,
  UpdateCheck,
  UpdateSettings,
  RestoredBackup,
  HumanDrop,
  MenuAction,
  LantaiStats,
  AgentSource,
  DayBoundary,
  AppIcon,
  BatchPlan,
  BatchStatusResult,
  DailyReportState,
  HumanMove,
  HumanNewPlace,
  HumanScope,
  HumanView,
  RemovedHumanNode,
  DailyResult,
  DayWorkbench,
  DaysResponse,
  EngineTestResult,
  Failure,
  LocalTool,
  ExportResult,
  Diagnostics,
  KeyUpdate,
  ModelInfo,
  ModelInput,
  ModelSettings,
  ProviderPreset,
  MonthsResponse,
  ProviderInput,
  ProjectDeleteImpact,
  ProjectRollup,
  ReportType,
  PeriodMode,
  RollupGranularity,
  WeeklyResult,
  WeeksResponse,
  ProjectWorkbench,
  ProjectsOverview,
  LanguageSettings,
  PromptItem,
  ReportProgress,
  ScanResult,
  SliceDetail,
  StatsRangeKey,
  StatsReport,
  SearchParams,
  SearchResults,
  StoredDigest,
} from "./types";

export const api = {
  scanSessions: (sinceDays?: number) =>
    invoke<ScanResult>("scan_sessions", { sinceDays: sinceDays ?? null }),
  /** Every day with sessions, newest first; ignored-only days only when asked for. */
  listDays: (includeIgnored: boolean) =>
    invoke<DaysResponse>("list_days", { includeIgnored }),
  dayWorkbench: (date: string) =>
    invoke<DayWorkbench>("day_workbench", { date }),
  listWeeks: () => invoke<WeeksResponse>("list_weeks"),
  /** The week's sessions; `date` of the result is the week key. */
  weekWorkbench: (week: string) =>
    invoke<DayWorkbench>("week_workbench", { week }),
  listMonths: () => invoke<MonthsResponse>("list_months"),
  /** The month's sessions; `date` of the result is the month key. */
  monthWorkbench: (month: string) =>
    invoke<DayWorkbench>("month_workbench", { month }),
  projectWorkbench: (
    projectId: number,
    sinceDate: string | null,
    untilDate: string | null,
  ) =>
    invoke<ProjectWorkbench>("project_workbench", {
      projectId,
      sinceDate,
      untilDate,
    }),
  sliceDetail: (sliceId: number) =>
    invoke<SliceDetail>("slice_detail", { sliceId }),
  setSessionNote: (sessionId: number, note: string | null) =>
    invoke<void>("set_session_note", { sessionId, note }),
  setSessionIgnored: (sessionId: number, ignored: boolean) =>
    invoke<void>("set_session_ignored", { sessionId, ignored }),
  setSliceIgnoreState: (
    sliceId: number,
    ignoreState: "inherit" | "ignored" | "included",
  ) => invoke<void>("set_slice_ignore_state", { sliceId, ignoreState }),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),
  listAppIcons: () => invoke<AppIcon[]>("list_app_icons"),
  setAppIcon: (id: string) => invoke<void>("set_app_icon", { id }),
  openSponsorPage: (channel: "githubSponsors" | "afdian") =>
    invoke<void>("open_sponsor_page", { channel }),
  updateSettings: () => invoke<UpdateSettings>("update_settings"),
  setCheckUpdates: (on: boolean) => invoke<void>("set_check_updates", { on }),
  /** Asks GitHub for the recent releases; nothing is downloaded. */
  checkForUpdates: () => invoke<UpdateCheck>("check_for_updates"),
  openReleasePage: (url: string) => invoke<void>("open_release_page", { url }),
  cliToolStatus: () => invoke<CliToolStatus>("cli_tool_status"),
  installCliTool: () => invoke<CliToolStatus>("install_cli_tool"),
  uninstallCliTool: () => invoke<CliToolStatus>("uninstall_cli_tool"),
  modelSettings: () => invoke<ModelSettings>("model_settings"),
  providerPresets: () => invoke<ProviderPreset[]>("provider_presets"),
  saveProvider: (
    originalName: string | null,
    name: string,
    provider: ProviderInput,
    key: KeyUpdate,
  ) =>
    invoke<ModelSettings>("save_provider", {
      originalName,
      name,
      provider,
      key,
    }),
  deleteProvider: (name: string) =>
    invoke<ModelSettings>("delete_provider", { name }),
  saveModel: (originalAlias: string | null, alias: string, model: ModelInput) =>
    invoke<ModelSettings>("save_model", { originalAlias, alias, model }),
  deleteModel: (alias: string) =>
    invoke<ModelSettings>("delete_model", { alias }),
  setDefaultModel: (alias: string) =>
    invoke<ModelSettings>("set_default_model", { alias }),
  listProviderModels: (
    savedName: string | null,
    provider: ProviderInput,
    apiKey: string | null,
  ) =>
    invoke<ModelInfo[]>("list_provider_models", { savedName, provider, apiKey }),
  testModel: (alias: string) =>
    invoke<EngineTestResult>("test_model", { alias }),
  /** Looks for known agent CLIs on PATH; runs only their version commands. */
  findLocalTools: () => invoke<LocalTool[]>("find_local_tools"),
  addLocalTool: (command: string) =>
    invoke<ModelSettings>("add_local_tool", { command }),
  diagnoseError: (message: string) =>
    invoke<Failure | null>("diagnose_error", { message }),

  listProjectsAndRules: () =>
    invoke<ProjectsOverview>("list_projects_and_rules"),
  createProject: (params: {
    name: string;
    description?: string | null;
    paths?: string[];
    remotes?: string[];
    regexes?: string[];
    regexField?: string;
    autoApply?: boolean;
  }) => invoke<number>("create_project", params),
  renameProject: (id: number, newName: string) =>
    invoke<void>("rename_project", { id, newName }),
  agentSources: () => invoke<AgentSource[]>("agent_sources"),
  getDayBoundary: () => invoke<DayBoundary>("get_day_boundary"),
  setDayBoundary: (hour: number) => invoke<DayBoundary>("set_day_boundary", { hour }),
  setAgentSettings: (agent: string, enabled: boolean, preDigest: boolean) =>
    invoke<AgentSource[]>("set_agent_settings", { agent, enabled, preDigest }),
  reorderProjects: (ids: number[]) => invoke<void>("reorder_projects", { ids }),
  describeProject: (id: number, description: string | null) =>
    invoke<void>("describe_project", { id, description }),
  deleteProject: (id: number) => invoke<void>("delete_project", { id }),
  projectDeleteImpact: (id: number) =>
    invoke<ProjectDeleteImpact>("project_delete_impact", { id }),
  setProjectArchived: (id: number, archived: boolean) =>
    invoke<void>("set_project_archived", { id, archived }),
  addRule: (params: {
    projectId: number;
    ruleType: string;
    targetField?: string | null;
    pattern: string;
    autoApply: boolean;
  }) => invoke<number>("add_rule", params),
  deleteRule: (id: number) => invoke<void>("delete_rule", { id }),
  setRuleAuto: (id: number, autoApply: boolean) =>
    invoke<void>("set_rule_auto", { id, autoApply }),
  assignSession: (sessionId: number, projectId: number | null) =>
    invoke<{ uncoveredPath: string | null }>("assign_session", {
      sessionId,
      projectId,
    }),
  assignSlice: (sliceId: number, projectId: number | null) =>
    invoke<void>("assign_slice", { sliceId, projectId }),
  unassignSession: (sessionId: number) =>
    invoke<void>("unassign_session", { sessionId }),
  acceptSuggestions: (params: { sessionIds: number[] }) =>
    invoke<number>("accept_suggestions", params),

  listPrompts: (kind?: string, allLanguages = false) =>
    invoke<PromptItem[]>("list_prompts", { kind: kind ?? null, allLanguages }),
  previewPrompt: (id: number, date?: string, project?: string) =>
    invoke<string>("preview_prompt", {
      id,
      date: date ?? null,
      project: project ?? null,
    }),
  createPrompt: (kind: string, name: string, body: string) =>
    invoke<number>("create_prompt", { kind, name, body }),
  updatePrompt: (id: number, body: string) =>
    invoke<void>("update_prompt", { id, body }),
  renamePrompt: (id: number, newName: string) =>
    invoke<void>("rename_prompt", { id, newName }),
  deletePrompt: (id: number) => invoke<void>("delete_prompt", { id }),
  setDefaultPrompt: (id: number) => invoke<void>("set_default_prompt", { id }),

  /** `systemLocales`: the system's preferred languages, most preferred first. */
  getLanguageSettings: (systemLocales: string[]) =>
    invoke<LanguageSettings>("get_language_settings", { systemLocales }),
  setUiLanguage: (language: string) => invoke<void>("set_ui_language", { language }),
  setReportLanguage: (language: string) => invoke<void>("set_report_language", { language }),
  /** Rebuild the native menu bar with these labels (`Messages["menu"]`). */
  setMenuLabels: (labels: Record<string, string>) =>
    invoke<void>("set_menu_labels", { labels }),

  getDailyReportState: (
    date: string,
    project: string | null,
    engine: string | null,
  ) =>
    invoke<DailyReportState>("get_daily_report_state", {
      date,
      project,
      engine,
    }),
  getProjectRollup: (params: {
    projectId: number;
    periodKey: string;
    granularity: RollupGranularity;
    sinceDate: string | null;
    untilDate: string | null;
  }) => invoke<ProjectRollup>("get_project_rollup", params),
  digestSliceNow: (sliceId: number, engine: string | null) =>
    invoke<StoredDigest>("digest_slice_now", { sliceId, engine }),
  getWeeklyReportState: (week: string, engine: string | null) =>
    invoke<DailyReportState>("get_weekly_report_state", { week, engine }),
  getMonthlyReportState: (month: string, engine: string | null) =>
    invoke<DailyReportState>("get_monthly_report_state", { month, engine }),
  generatePeriodReport: (
    mode: PeriodMode,
    key: string,
    options: { prompt: string | null; engine: string | null; update: boolean },
  ) => {
    switch (mode) {
      case "daily":
        return invoke<DailyResult>("generate_daily_report", { date: key, project: null, ...options });
      case "weekly":
        return invoke<WeeklyResult>("generate_weekly_report", { week: key, ...options });
      case "monthly":
        return invoke<WeeklyResult>("generate_monthly_report", { month: key, ...options });
    }
  },
  /** Cancel the job of a period: a date or a week key. */
  cancelReportGeneration: (period: string, project: string | null = null) =>
    invoke<void>("cancel_report_generation", { period, project }),
  /** Keys (a date, a week key, or "date|project") of report jobs running in the backend. */
  runningReportJobs: () => invoke<string[]>("running_report_jobs"),
  saveReportEdit: (params: {
    reportType: ReportType;
    periodKey: string;
    projectId: number | null;
    markdown: string;
  }) =>
    invoke<number>("save_report_edit", {
      reportType: params.reportType,
      periodKey: params.periodKey,
      projectId: params.projectId,
      project: null,
      markdown: params.markdown,
    }),
  setReportStatus: (params: {
    reportType: ReportType;
    periodKey: string;
    projectId: number | null;
    status: "draft" | "final";
  }) =>
    invoke<boolean>("set_report_status", {
      reportType: params.reportType,
      periodKey: params.periodKey,
      projectId: params.projectId,
      project: null,
      status: params.status,
    }),
  planBatch: (reportType: PeriodMode, periodKeys: string[], includeFinal: boolean) =>
    invoke<BatchPlan>("plan_batch", { reportType, periodKeys, includeFinal }),
  setReportsStatus: (reportType: PeriodMode, periodKeys: string[], status: "draft" | "final") =>
    invoke<BatchStatusResult>("set_reports_status", { reportType, periodKeys, status }),
  restoreReportVersion: (params: {
    reportType: ReportType;
    periodKey: string;
    projectId: number | null;
    versionId: number;
  }) =>
    invoke<boolean>("restore_report_version", {
      reportType: params.reportType,
      periodKey: params.periodKey,
      projectId: params.projectId,
      project: null,
      versionId: params.versionId,
    }),
  exportReport: (params: {
    reportType: ReportType;
    periodKey: string;
    projectName: string | null;
    markdown: string;
    customPath: string | null;
    anchorMode: "keep" | "footnotes" | "strip";
    force: boolean;
  }) => invoke<ExportResult>("export_daily_report", params),
  /** Asks where to save; `null` when the user cancels. */
  exportBackup: (prompt: string) => invoke<BackupInfo | null>("export_backup", { prompt }),
  /** Asks for a backup file and describes it; `null` when the user cancels. */
  pickBackup: (prompt: string) => invoke<BackupInfo | null>("pick_backup", { prompt }),
  restoreBackup: (path: string) => invoke<RestoredBackup>("restore_backup", { path }),
  pickExportDirectory: (defaultDir?: string | null) =>
    invoke<string | null>("pick_export_directory", {
      defaultDir: defaultDir ?? null,
    }),
  getStats: (range: StatsRangeKey, includeIgnored?: boolean) =>
    invoke<StatsReport>("get_stats", {
      range,
      includeIgnored: includeIgnored ?? false,
    }),
  searchAll: (params: SearchParams) => invoke<SearchResults>("search_all", { params }),

  humanLoad: (scope: HumanScope) => invoke<HumanView>("human_load", { scope }),
  humanCreate: (scope: HumanScope, place: HumanNewPlace, body: string) =>
    invoke<{ id: number; view: HumanView }>("human_create", { scope, place, body }),
  humanUpdate: (scope: HumanScope, id: number, body: string) =>
    invoke<HumanView>("human_update", { scope, id, body }),
  humanMove: (scope: HumanScope, id: number, direction: HumanMove) =>
    invoke<HumanView>("human_move", { scope, id, direction }),
  humanMoveTo: (scope: HumanScope, ids: number[], drop: HumanDrop) =>
    invoke<HumanView>("human_move_to", { scope, ids, drop }),
  humanDelete: (scope: HumanScope, ids: number[]) =>
    invoke<{ removed: RemovedHumanNode[]; view: HumanView }>("human_delete", { scope, ids }),
  humanRestore: (scope: HumanScope, rows: RemovedHumanNode[]) =>
    invoke<HumanView>("human_restore", { scope, rows }),
  /** Open a link written in a note (http and https only) in the system browser. */
  openNoteLink: (url: string) => invoke<void>("open_note_link", { url }),
  getLantaiStats: (range: StatsRangeKey) => invoke<LantaiStats>("get_lantai_stats", { range }),

  onReportProgress: (
    handler: (p: { key: string; progress: ReportProgress }) => void,
  ): Promise<UnlistenFn> =>
    listen<{ key: string; progress: ReportProgress }>("report-progress", (ev) =>
      handler(ev.payload),
    ),
  onReportJobFinished: (handler: (key: string) => void): Promise<UnlistenFn> =>
    listen<string>("report-job-finished", (ev) => handler(ev.payload)),
  onMenuAction: (handler: (action: MenuAction) => void): Promise<UnlistenFn> =>
    listen<MenuAction>("menu-action", (ev) => handler(ev.payload)),
};
