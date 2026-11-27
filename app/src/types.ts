export interface ScanResult {
  filesSeen: number;
  filesParsed: number;
  filesUnchanged: number;
  filesFailed: number;
  newTurns: number;
  newItems: number;
  assignedByRule: number;
  suggestionsPending: number;
  failures: string[];
}

export interface DayOverview {
  localDate: string;
  slices: number;
  ignoredSlices: number;
  activeMinutes: number;
  unassignedSlices: number;
  reportStatus: "draft" | "final" | null;
  /** Top-level notes and annotations people wrote for the day. */
  notes: number;
}

export interface DaysResponse {
  today: string;
  days: DayOverview[];
}

export interface WeekOverview {
  /** ISO week key, e.g. "2026-W47". */
  week: string;
  monday: string;
  sunday: string;
  activeDays: number;
  slices: number;
  activeMinutes: number;
  unassignedSlices: number;
  reportStatus: "draft" | "final" | null;
}

export interface WeeksResponse {
  currentWeek: string;
  weeks: WeekOverview[];
}

export interface MonthOverview {
  /** Calendar month key, e.g. "2026-11". */
  month: string;
  firstWeek: string;
  lastWeek: string;
  activeWeeks: number;
  reportedWeeks: number;
  activeDays: number;
  slices: number;
  activeMinutes: number;
  unassignedSlices: number;
  reportStatus: "draft" | "final" | null;
}

export interface MonthsResponse {
  currentMonth: string;
  months: MonthOverview[];
}

export interface WeeklyResult {
  slicesUsed: number;
  projectsWritten: string[];
  sectionsKept: number;
  updated: boolean;
  warnings: string[];
  anchors: { total: number; invalid: number };
  versionId: number | null;
}

/** Report kinds the report commands accept. */
export type PeriodMode = "daily" | "weekly" | "monthly";
export type ReportType = PeriodMode | "project";

export interface SliceCard {
  id: number;
  sessionId: number;
  sourceSessionId: string;
  shortId: string;
  /** Stable agent key ("claude", "codex", ...); `agentName` is its display name. */
  agent: string;
  agentName: string;
  title: string;
  projectId: number | null;
  projectName: string | null;
  projectOverrideId: number | null;
  assignmentSource: string;
  suggestedProjectId: number | null;
  suggestedProjectName: string | null;
  suggestionSource: string | null;
  suggestionReason: string | null;
  uncoveredPath: string | null;
  sliceDate: string;
  startTime: string;
  endTime: string;
  activeMinutes: number;
  turnCount: number;
  filesModifiedCount: number;
  userNote: string | null;
  ignored: boolean;
  sessionIgnored: boolean;
  ignoreState: "inherit" | "ignored" | "included";
  hasDigest: boolean;
  /** When the slice's last turn ended (RFC 3339); recent slices are still live. */
  endedAt: string;
}

export interface ProjectOption {
  id: number;
  name: string;
  description: string | null;
  /** Archived projects are not offered for new assignments. */
  archived: boolean;
}

export interface PromptOption {
  id: number;
  name: string;
  isDefault: boolean;
  isBuiltin: boolean;
}

export interface DayWorkbench {
  date: string;
  slices: SliceCard[];
  projects: ProjectOption[];
  prompts: PromptOption[];
  /** Null when no model is configured. */
  defaultEngine: string | null;
  engines: string[];
  /** Slices that ended more recently than this are live and are not pre-digested. */
  quietMinutes: number;
}

export interface ProjectWorkbench {
  project: ProjectSummary;
  slices: SliceCard[];
  suggestedSlices: SliceCard[];
  rules: RuleItem[];
  projects: ProjectOption[];
}

/** Which period reports a project rollup is made of. */
export type RollupGranularity = PeriodMode;

export interface ProjectRollupEntry {
  /** Period key: a date, or a week key such as "2026-W47". */
  period: string;
  /** Readable period: the date, or "2026-W47 · 11-16 ~ 11-22". */
  label: string;
  /** Empty when the period only has notes. */
  status: "draft" | "final" | "";
  markdown: string;
  updatedAt: string;
}

export interface ProjectRollup {
  projectId: number;
  projectName: string;
  periodKey: string;
  granularity: RollupGranularity;
  entries: ProjectRollupEntry[];
  unreportedPeriods: string[];
  combinedMarkdown: string;
  defaultExportPath: string;
}

export interface DigestPoint {
  text: string;
  turns: number[];
}

export interface DigestSummary {
  kind: "summary";
  goal: string;
  key_changes: DigestPoint[];
  decisions_and_pitfalls: DigestPoint[];
  outcome: string;
  open_items: string[];
  tags: string[];
  suggested_project?: string | null;
}

export interface DigestRaw {
  kind: "raw";
  text: string;
}

export type StoredDigest = DigestSummary | DigestRaw;

export interface ToolDetail {
  kind: string;
  toolName: string;
  target: string | null;
  summary: string | null;
  isError: boolean | null;
  added: number | null;
  removed: number | null;
  outputSummary: string | null;
}

export interface TurnDetail {
  turnIndex: number;
  time: string;
  userPrompt: string;
  conclusion: string | null;
  messages: string[];
  tools: ToolDetail[];
}

export interface SliceDetail {
  slice: SliceCard;
  detectedCwd: string | null;
  gitRoot: string | null;
  gitRemote: string | null;
  gitBranch: string | null;
  filesModified: string[];
  dehydratedChars: number;
  digest: StoredDigest | null;
  /** Which model wrote `digest`; null when unknown (written before Lantai recorded it). */
  digestModel: ModelRef | null;
  turns: TurnDetail[];
}

/** An engine name as stored with a digest, and the model aliases that produce it now. */
export interface ModelRef {
  engine: string;
  /** Empty when no alias produces this engine any more. */
  aliases: string[];
}

export interface ProjectSummary {
  id: number;
  name: string;
  description: string | null;
  archived: boolean;
  slices: number;
  suggested: number;
  rules: number;
}

export interface RuleItem {
  id: number;
  projectId: number;
  projectName: string;
  ruleType: "path_prefix" | "git_remote" | "regex";
  targetField: string | null;
  pattern: string;
  autoApply: boolean;
}

export interface ProjectCandidate {
  root: string;
  gitRemote: string | null;
  sessions: number;
  lastActiveAt: string | null;
  sampleTitles: string[];
}

export interface ProjectsOverview {
  projects: ProjectSummary[];
  rules: RuleItem[];
  candidates: ProjectCandidate[];
}

export interface PromptItem {
  id: number;
  kind: string;
  /** The report language the prompt is tuned for. */
  language: string;
  name: string;
  body: string;
  isDefault: boolean;
  isBuiltin: boolean;
  updatedAt: string;
  assembledPreview: string;
}

export interface DailyEstimate {
  slicesUsed: number;
  slicesIgnored: number;
  digestsCached: number;
  digestsPending: number;
  estimatedChars: number;
  /** Set when the current version was generated from different material than there is now. */
  staleness: Staleness | null;
  /** Active weeks in a month that lack a weekly report (monthly reports only). */
  unreportedWeeks?: string[];
}

export interface Staleness {
  newSlices: number;
  changedSlices: number;
  removedSlices: number;
  /** Projects whose sections an update would rewrite or drop. */
  projects: string[];
}

export interface ReportMeta {
  status: "draft" | "final";
  currentVersionId: number | null;
}

export interface ReportVersion {
  id: number;
  number: number;
  origin: "generated" | "user_edit";
  markdown: string;
  engine: string | null;
  /** The model aliases that produce `engine` now (empty when none does). */
  models: string[];
  promptName: string | null;
  promptSnapshot: string | null;
  /** The language the version is written in. */
  language: string;
  createdAt: string;
  isCurrent: boolean;
  /** Lines the user wrote rather than the model (current version only). */
  edits: ReportEdits | null;
}

/** How the displayed report differs from what the model wrote; lines index `markdown.split("\n")`. */
export interface ReportEdits {
  lines: LineEdit[];
  removed: RemovedLines[];
}

export interface LineEdit {
  line: number;
  /** The generated line it rewrote; null for a line the user added. */
  original: string | null;
}

export interface RemovedLines {
  headingLine: number;
  lines: string[];
}

export interface DailyReportState {
  estimate: DailyEstimate;
  meta: ReportMeta | null;
  versions: ReportVersion[];
  defaultExportPath: string;
}

export interface DailyResult {
  markdown: string;
  slicesUsed: number;
  slicesIgnored: number;
  digestsCached: number;
  digestsGenerated: number;
  digestsIncremental: number;
  digestsPassthrough: number;
  slicesSplit: number;
  partsGenerated: number;
  partsCached: number;
  projectsCondensed: number;
  briefsCached: number;
  suggestionsMade: number;
  updated: boolean;
  projectsWritten: string[];
  sectionsKept: number;
  warnings: string[];
  anchors: { total: number; invalid: number };
  stage2PromptChars: number;
  versionId: number | null;
  promptName: string;
}

export interface ExportResult {
  status: "written" | "conflict";
  path: string;
  bytes: number | null;
  existingMarkdown: string | null;
}

export interface SourceHealth {
  agent: string;
  files: number;
  errorCount: number;
  unknownCount: number;
  lastError: string | null;
  agentVersions: string[];
}

export interface Diagnostics {
  dataDir: string;
  exportDir: string;
  irVersion: string;
  sessions: number;
  turns: number;
  sources: SourceHealth[];
}

export interface AppIcon {
  id: string;
  label: string;
  current: boolean;
}

export type ProviderKind = "openai_compat" | "claude_cli" | "codex_cli";

/** Whether a provider kind is an agent CLI on this Mac (no key, a command instead of a URL). */
export const isCliKind = (kind: ProviderKind | undefined): boolean =>
  kind === "claude_cli" || kind === "codex_cli";

/** A model a provider offers (`aj_report::ModelInfo`). */
export interface ModelInfo {
  id: string;
  /** Reasoning effort values it accepts; empty when unknown. */
  efforts: string[];
  /** The effort used when none is set, when known. */
  defaultEffort: string | null;
}

/** A well-known service that fills the provider form (`aj_report::presets`). */
export interface ProviderPreset {
  /** Suggested provider name. */
  id: string;
  name: string;
  group: "vendor" | "aggregator" | "local";
  /** Set only when one vendor runs separate global and mainland-China sites. */
  region: "global" | "china" | null;
  baseUrl: string;
  /** `reasoning_effort` hints; null means the generic list applies. */
  efforts: string[] | null;
}

export interface ProviderSetting {
  name: string;
  kind: ProviderKind;
  baseUrl: string | null;
  timeoutSecs: number | null;
  binary: string | null;
  /** A key is in the key store; the key itself never reaches the frontend. */
  hasKey: boolean;
  usedBy: string[];
}

export interface ModelSetting {
  alias: string;
  provider: string;
  model: string | null;
  reasoningEffort: string | null;
  temperature: number | null;
  isDefault: boolean;
}

export interface ModelSettings {
  configPath: string;
  keyStore: string;
  /** Null when no model is configured. */
  defaultModel: string | null;
  providers: ProviderSetting[];
  models: ModelSetting[];
}

export interface ProviderInput {
  kind: ProviderKind;
  baseUrl: string | null;
  timeoutSecs: number | null;
  binary: string | null;
}

export interface ModelInput {
  provider: string;
  model: string | null;
  reasoningEffort: string | null;
  /** Only for OpenAI-compatible providers. */
  temperature: number | null;
}

export type KeyUpdate =
  { mode: "keep" } | { mode: "set"; value: string } | { mode: "clear" };

/** A known kind of engine error (`aj_report::diagnose`). */
export type Failure = "no_model" | "command_not_found" | "not_signed_in" | "quota";

export interface EngineTestResult {
  ok: boolean;
  elapsedMs: number;
  message: string;
  failure: Failure | null;
}

/** An agent CLI found on this Mac (`aj_report::local_tools`). */
export interface LocalTool {
  command: string;
  agent: string;
  product: string;
  path: string;
  version: string | null;
  /** Null when it cannot generate reports; `gaps` says why. */
  engine: string | null;
  gaps: string[];
  /** The model alias already using this exact command. */
  configuredAs: string | null;
}

export type ViewMode = "daily" | "weekly" | "monthly" | "projects" | "stats" | "search";
/** Payload of the backend's `menu-action` event (native menu bar clicks and shortcuts). */
export type MenuAction =
  | { kind: "view"; mode: ViewMode }
  | { kind: "settings" }
  | { kind: "about" }
  | { kind: "checkUpdates" };
export type ProjectRangeKey = "7d" | "14d" | "30d" | "all";
export type StatsRangeKey = "7d" | "30d" | "90d" | "all";
export type SettingsTab =
  | "appearance"
  | "language"
  | "prompts"
  | "projects"
  | "rules"
  | "models"
  | "agents"
  | "system"
  | "about";
export type ThemeId =
  | "github-light"
  | "solarized-light"
  | "catppuccin-latte"
  | "one-dark"
  | "tokyo-night"
  | "catppuccin-mocha";
export type DensityId = "compact" | "default" | "comfortable";

/** Phase of a running report job: digesting slices, then writing the report. */
export type GenStage = "stage1" | "stage2";

export type ReportBatchMode = "smart" | "stale" | "missing" | "regenerate";

export interface BatchItem {
  periodKey: string;
  hasInputs: boolean;
  missingWeeks: string[];
  status: "none" | "draft" | "final";
  stale: boolean;
  action: "generate" | "update" | "skip";
  canRegenerate: boolean;
}

export interface BatchPlan {
  items: BatchItem[];
  missingCount: number;
  staleCount: number;
  regenerateCount: number;
  batchConcurrency: number;
}

export interface BatchStatusResult {
  updated: string[];
  skipped: string[];
}

export interface ReportBatchRun {
  done: number;
  total: number;
}

/** A coding agent's session source and its settings (Settings > Agents). */
/** Local hour a new day starts; activity before it counts towards the previous day. */
export interface DayBoundary {
  hour: number;
  maxHour: number;
}

export interface AgentSource {
  agent: string;
  name: string;
  roots: { path: string; exists: boolean }[];
  detected: boolean;
  enabled: boolean;
  preDigest: boolean;
  /** Whether the local data's format (agent version) is one the adapter reads. */
  compatibility: "unchecked" | "supported" | "partial" | "unsupported";
  /** Agent versions seen in the local data. */
  versions: string[];
  /** Why the local data is not fully read (or why checking failed). */
  compatReason: CompatReason | null;
  /** Sessions already stored; kept (and no longer updated) when the agent is disabled. */
  storedSessions: number;
}

export type CompatReason =
  | { kind: "newerFormat" }
  | { kind: "sessionsInNewerFormat"; count: number }
  | { kind: "untestedVersions"; versions: string[] }
  | { kind: "unrecognizedLayout" }
  | { kind: "probeFailed"; message: string };

export interface StatsTrendSegment {
  projectId: number | null;
  name: string;
  activeMinutes: number;
}

export interface StatsTrendBucket {
  key: string;
  label: string;
  dateRange: string;
  activeMinutes: number;
  sessions: number;
  topProject: string | null;
  segments: StatsTrendSegment[];
  tokens: TokenUsage;
}

/** Four non-overlapping token counts; `inputTokens` excludes cache reads and writes. */
export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
}

export interface TokenModelStat {
  agent: string;
  agentName: string;
  model: string | null;
  calls: number;
  usage: TokenUsage;
  sharePct: number;
}

export interface TokenAgentStat {
  agent: string;
  name: string;
  calls: number;
  usage: TokenUsage;
  sharePct: number;
}

export interface TokenStats {
  calls: number;
  usage: TokenUsage;
  cacheHitPct: number;
  models: TokenModelStat[];
  agents: TokenAgentStat[];
}

export interface StatsProjectItem {
  projectId: number | null;
  name: string;
  activeMinutes: number;
  sharePct: number;
  activeDays: number;
  sessions: number;
  turns: number;
  filesChanged: number;
}

export interface StatsAgentItem {
  agent: string;
  name: string;
  activeMinutes: number;
  sharePct: number;
  sessions: number;
  turns: number;
  filesChanged: number;
}

export interface StatsToolCategory {
  key: string;
  label: string;
  calls: number;
  errors: number;
  sharePct: number;
}

export interface StatsHourlyBucket {
  hour: number;
  activeMinutes: number;
}

export interface StatsReport {
  range: StatsRangeKey;
  sinceDate: string | null;
  untilDate: string;
  rangeDays: number;
  activeMinutes: number;
  activeDays: number;
  currentStreakDays: number;
  longestStreakDays: number;
  sessions: number;
  slices: number;
  turns: number;
  filesChanged: number;
  linesAdded: number;
  linesRemoved: number;
  trendGranularity: "day" | "week";
  trend: StatsTrendBucket[];
  projects: StatsProjectItem[];
  agents: StatsAgentItem[];
  totalToolCalls: number;
  tools: StatsToolCategory[];
  hourly: StatsHourlyBucket[];
  peakHour: number | null;
  lateNightPct: number;
  tokens: TokenStats;
}

/** Report state of one day, week or month in the Lantai section. */
export type LantaiCellStatus = "none" | "draft" | "final" | "ignored";

export interface LantaiPeriodCell {
  key: string;
  /** Days the period covers (a month spans its weeks). */
  firstDay: string;
  lastDay: string;
  status: LantaiCellStatus;
  stale: boolean;
  staleProjects: string[];
  versions: number;
  edited: boolean;
}

export interface LantaiKindTotals {
  /** Periods that need a report (not entirely ignored). */
  periods: number;
  reports: number;
  finals: number;
  stale: number;
  edited: number;
}

export interface LantaiDigestTally {
  fresh: number;
  behind: number;
  missing: number;
  ignored: number;
}

export interface LantaiStats {
  /** The window's first and last day. */
  from: string;
  to: string;
  days: LantaiPeriodCell[];
  weeks: LantaiPeriodCell[];
  months: LantaiPeriodCell[];
  daily: LantaiKindTotals;
  weekly: LantaiKindTotals;
  monthly: LantaiKindTotals;
  digestDays: { date: string; tally: LantaiDigestTally }[];
  digest: LantaiDigestTally;
  agents: { agent: string; name: string; tally: LantaiDigestTally }[];
  splitSlices: number;
  incrementalUpdates: number;
  projectBriefs: number;
  /** Engine the reusable check was made for; empty when no model is configured. */
  engine: string;
}

/** A language reports can be written in. */
export interface ReportLanguage {
  tag: string;
  nativeName: string;
  /** Whether it has its own tuned prompts (otherwise it borrows the English ones). */
  tuned: boolean;
}

export interface LanguageSettings {
  /** "auto" or a language tag. */
  uiLanguage: string;
  reportLanguage: string;
  reportLanguages: ReportLanguage[];
}

/** One step of a report generation (`aj_report::progress::Progress`). */
export type ReportProgress =
  | { stage: "digestsPending"; pending: number; cached: number }
  | {
      stage: "sliceDigested";
      sliceId: number;
      title: string;
      parts: number | null;
      incremental: boolean;
    }
  | { stage: "clusteringProjects" }
  | { stage: "clusteringDays" }
  | { stage: "writingDaily"; sections: number | null; chars: number }
  | { stage: "writingProjects"; count: number; concurrency: number }
  | { stage: "projectWritten"; project: string; ok: boolean }
  | { stage: "repairingStructure" };

// ---- Search -------------------------------------------------------------

export type SearchLayer = "all" | "reports" | "sessions";

export interface SearchParams {
  query: string;
  layer: SearchLayer;
  since: string | null;
  until: string | null;
  projectId: number | null;
  agent: string | null;
  includeIgnored: boolean;
  includeToolOutput: boolean;
}

export interface SnippetPart {
  text: string;
  hit: boolean;
}

export interface FieldSnippet {
  /** `prompt` | `reply` | `tool` | `tool_output` | `title` | `note` */
  field: string;
  parts: SnippetPart[];
}

export interface ReportRef {
  kind: string;
  periodKey: string;
}

export interface ReportSearchHit {
  kind: string;
  periodKey: string;
  status: string;
  section: string | null;
  snippets: SnippetPart[][];
  matchCount: number;
  /** A note people wrote rather than a report section. */
  human: boolean;
}

export interface TurnSearchHit {
  turnIndex: number;
  startedAt: string;
  snippets: FieldSnippet[];
  matchCount: number;
  citedBy: ReportRef[];
}

export interface SliceSearchHit {
  sliceId: number;
  localDate: string;
  sessionId: number;
  agent: string;
  agentName: string;
  title: string | null;
  projectId: number | null;
  projectName: string | null;
  ignored: boolean;
  titleSnippets: FieldSnippet[];
  turns: TurnSearchHit[];
  matchCount: number;
  citedBy: ReportRef[];
}

export interface SearchResults {
  terms: string[];
  reports: ReportSearchHit[];
  reportsTruncated: boolean;
  sessions: SliceSearchHit[];
  sessionsTruncated: boolean;
  elapsedMs: number;
}

/** A note or annotation someone wrote next to a report (see `cmd_human.rs`). */
/** Where a project note shows: before or after the generated points of its section. */
export type HumanPosition = "top" | "bottom";

export interface HumanNode {
  id: number;
  /** `day` | `week` | `month`, or `project` for the notes of a project's own page. */
  periodKind: string;
  periodKey: string;
  projectId: number | null;
  parentId: number | null;
  sortKey: string;
  /** Markdown. */
  body: string;
  /** A root written on a point of the generated report. */
  isAnnotation: boolean;
  /** Top or bottom of the project's section (children copy their root's). */
  position: HumanPosition;
  /** The point's text when the annotation was written. */
  quote: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Where an annotation shows: under the point on `line`, at the end of the section whose heading is on `line`, or apart from the report. */
export interface HumanPlacement {
  id: number;
  kind: "point" | "section" | "detached";
  line: number | null;
}

export interface HumanView {
  nodes: HumanNode[];
  placements: HumanPlacement[];
}

/**
 * The nodes a view works on: one report period, or a project page (its own notes and the
 * project's notes of every `granularity` period).
 */
export type HumanScope =
  | { type: "period"; kind: ReportType; period: string }
  | { type: "project"; projectId: number; granularity: RollupGranularity };

/** In a project scope, `period` names the period a note or annotation is for (none: the page). */
export type HumanNewPlace =
  | { type: "dayNote" }
  | { type: "projectNote"; projectId: number; period?: string; position?: HumanPosition }
  | { type: "annotation"; line: number; period?: string }
  | { type: "after"; id: number }
  | { type: "lastChild"; id: number };

/** Where dragged nodes are dropped; `after` is the sibling they go right after (the start when null). */
export type HumanDrop =
  | { type: "notes"; projectId: number | null; position: HumanPosition; after: number | null }
  | { type: "annotations"; with: number; after: number | null }
  | { type: "children"; parent: number; after: number | null };

export type HumanMove = "up" | "down" | "indent" | "outdent";

/** A deleted node exactly as stored, kept for an undo. */
export interface RemovedHumanNode {
  id: number;
  periodKind: string;
  periodKey: string;
  projectId: number | null;
  parentId: number | null;
  sortKey: string;
  attachJson: string | null;
  position: HumanPosition;
  body: string;
  createdAt: string;
  updatedAt: string;
}

/** Which layer of the journal a report shows. */
export type SourceFilter = "both" | "agent" | "human";

/** What deleting a project would affect. */
export interface ProjectDeleteImpact {
  sessions: number;
  reports: number;
  rules: number;
  notes: number;
}

/** A backup archive of the database and the settings. */
export interface UpdateSettings {
  checkUpdates: boolean;
  currentVersion: string;
}

/** The result of a check against GitHub Releases. */
export type UpdateCheck =
  | { status: "upToDate"; current: string }
  | {
      status: "available";
      current: string;
      latest: string;
      /** Release page of the latest version. */
      url: string;
      publishedAt: string | null;
      /** Whether a release since the running one lists breaking changes. */
      breaking: boolean;
    };

export type CliLinkState = "missing" | "installed" | "staleLink" | "other";

export interface CliToolStatus {
  /** Whether this build carries the `aj` binary. */
  bundled: boolean;
  /** Whether macOS runs the app from a temporary location (not moved to Applications yet). */
  translocated: boolean;
  state: CliLinkState;
  /** The link location, with the home folder written as `~`. */
  linkPath: string;
  /** Whether the link's folder is on the login shell's PATH. */
  onPath: boolean;
}

export interface BackupInfo {
  path: string;
  appVersion: string;
  schemaVersion: number;
  /** RFC 3339, UTC. */
  createdAt: string;
  /** The configuration files in the backup. */
  files: string[];
}

export interface RestoredBackup {
  backup: BackupInfo;
  /** Where the data that was replaced is kept. */
  previous: string;
}

/** Whether the database can be used as it is (docs/SPEC.md, "Upgrading the database"). */
export type DatabaseState =
  | { state: "ready" }
  | { state: "outdated"; from: number; to: number }
  | { state: "tooOld"; found: number }
  | { state: "tooNew"; found: number; known: number };

export interface DatabaseStatus {
  database: DatabaseState;
  /** The interface language setting ("auto" or a tag), read without the database. */
  uiLanguage: string;
  backupDir: string;
  dataDir: string;
  releaseUrl: string;
}

export interface UpgradedDatabase {
  from: number;
  to: number;
  backup: string;
}
