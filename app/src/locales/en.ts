// English interface texts. This file is the source of every key: other languages are
// checked against `Messages` and fall back to these texts for keys they leave out.
// Texts that depend on a number are functions, written per language.

import type { ReportProgress } from "../types";

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
/** "day" / "week" / "month" for a granularity ("daily", "weekly", "monthly"). */
const unitOf = (g: string) => (g === "monthly" ? "month" : g === "weekly" ? "week" : "day");
/** "daily report" / "weekly report" / "monthly report" for a granularity. */
const reportOf = (g: string) => `${g} report`;

export const en = {
  meta: {
    nativeName: "English",
    /** The app's name: the window title and the `{name}` in menu items. */
    appName: "Lantai",
    /** Joins short list items ("a, b"). */
    listSeparator: ", ",
  },

  /** Native menu bar; `{name}` is the app's name. */
  menu: {
    about: "About {name}",
    checkUpdates: "Check for Updates…",
    settings: "Settings…",
    services: "Services",
    hide: "Hide {name}",
    hideOthers: "Hide Others",
    showAll: "Show All",
    quit: "Quit {name}",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    selectAll: "Select All",
    view: "View",
    daily: "Daily",
    weekly: "Weekly",
    monthly: "Monthly",
    projects: "Projects",
    statistics: "Statistics",
    search: "Search",
    fullScreen: "Enter Full Screen",
    window: "Window",
    minimize: "Minimize",
    zoom: "Zoom",
    showMainWindow: "Show Main Window",
    bringAllToFront: "Bring All to Front",
    closeWindow: "Close Window",
    help: "Help",
  },

  /** One step of a report generation, as the backend reports it. */
  progress: (p: ReportProgress): string => {
    switch (p.stage) {
      case "digestsPending":
        return `Summarizing ${plural(p.pending, "session", "sessions")} (${p.cached} cached)`;
      case "sliceDigested":
        return `S${p.sliceId} ${p.title}${p.parts ? ` (${p.parts} parts)` : p.incremental ? " (new turns added)" : ""}`;
      case "clusteringProjects":
        return "Grouping busy projects into tasks";
      case "clusteringDays":
        return "Grouping busy days into tasks";
      case "writingDaily":
        return p.sections == null
          ? "Writing the daily report"
          : `Rewriting ${plural(p.sections, "changed section", "changed sections")}`;
      case "writingProjects":
        return `Writing ${plural(p.count, "project section", "project sections")}`;
      case "projectWritten":
        return `${p.project}: ${p.ok ? "written" : "failed"}`;
      case "repairingStructure":
        return "Fixing the report structure";
    }
  },

  common: {
    create: "Create",
    edit: "Edit",
    cancel: "Cancel",
    save: "Save",
    saved: "Saved",
    delete: "Delete",
    rename: "Rename",
    close: "Close",
    newItem: "+ New",
    loading: "Loading...",
    optional: "(optional)",
    reportStatus: (status: string | null): string =>
      status === "final" ? "Final" : status === "draft" ? "Draft" : "No report yet",
    generating: (stage: "stage1" | "stage2"): string =>
      stage === "stage2" ? "Writing report..." : "Summarizing sessions...",
  },

  sidebar: {
    scan: "Scan",
    scanning: "Scanning...",
    stats: "Statistics",
    search: "Search",
    settings: "Settings",
    projects: "Projects",
    projectName: "Project name",
    projectDescription: "Description (optional)",
    unassigned: (n: number) => `${plural(n, "unassigned slice", "unassigned slices")}`,
    noWeeks: "No active weeks yet.",
    noMonths: "No active months yet.",
    thisWeek: "This week",
    thisMonth: "This month",
    weekStatsTitle: (days: number, slices: number, minutes: number) =>
      `${plural(days, "active day", "active days")}, ${plural(slices, "slice", "slices")}, ${minutes} active minutes`,
    weekStats: (days: number, time: string) => `${days}d · ${time}`,
    monthStatsTitle: (reported: number, weeks: number, days: number, slices: number, minutes: number) =>
      `${reported}/${weeks} weekly reports, ${plural(days, "active day", "active days")}, ${plural(slices, "slice", "slices")}, ${minutes} active minutes`,
    monthStats: (reported: number, weeks: number, time: string) => `${reported}/${weeks}w · ${time}`,
  },

  calendar: {
    week: "W",
    daySummary: (date: string, slices: number, minutes: number, unassigned: number) =>
      `${date}: ${plural(slices, "slice", "slices")}, ${minutes} min${unassigned > 0 ? `, ${unassigned} unassigned` : ""}`,
    weekSummary: (key: string, days: number, slices: number, minutes: number, status: string | null) =>
      `${key}: ${plural(days, "active day", "active days")}, ${plural(slices, "slice", "slices")}, ${minutes} min${status ? `, report ${status}` : ""}`,
    monthSummary: (key: string, reported: number, weeks: number, slices: number, minutes: number) =>
      `${key}: ${reported}/${weeks} weekly reports, ${plural(slices, "slice", "slices")}, ${minutes} min`,
    today: "Today",
    thisWeek: "This week",
    thisMonth: "This month",
    jumpToToday: "Jump to today",
    jumpTo: (label: string, period: string) => `Jump to ${label.toLowerCase()} (${period})`,
    previousMonth: "Previous month",
    nextMonth: "Next month",
    previousYear: "Previous year",
    nextYear: "Next year",
  },

  dayList: {
    days: "Days",
    options: "Daily list options",
    showIgnoredDays: "Show ignored days",
    selectDays: "Select days...",
    exitMultiSelect: "Exit multi-select",
    quickSelect: "Quick select",
    thisWeek: "This week",
    thisMonth: "This month",
    allDraft: "All drafts",
    allOutOfDate: "All out of date",
    allWithoutReport: "All without a report",
    selectDay: (date: string) => `Select ${date}`,
    today: "Today",
    unassignedBadge: (n: number) => `${n} unassigned`,
    ignoredTitle: (n: number) => plural(n, "ignored slice", "ignored slices"),
    ignored: (n: number) => `${n} ignored`,
    statsTitle: (slices: number, minutes: number) =>
      `${plural(slices, "slice", "slices")}, ${minutes} active minutes`,
    notesTitle: (n: number) => plural(n, "note", "notes"),
  },

  granularity: {
    short: { daily: "Day", weekly: "Week", monthly: "Month" } as Record<string, string>,
    tip: { daily: "By day", weekly: "By week", monthly: "By month" } as Record<string, string>,
  },

  views: {
    daily: "Daily",
    weekly: "Weekly",
    monthly: "Monthly",
    projects: "Projects",
    stats: "Statistics",
    search: "Search",
  },

  outline: {
    collapse: "Collapse details",
    expand: "Expand details",
    collapseTitle: "Show the overview only",
    expandTitle: "Show every detail line",
  },

  human: {
    filters: { both: "Agent and you", agent: "Agent only", human: "Only what you wrote" },
    filterTitle: "Show the generated report, what you wrote, or both",
    dayNotes: "Notes",
    addDayNote: "Add a note",
    addNote: "Note",
    noteAtTop: "At the top",
    dropHere: "Drop here",
    noteAtBottom: "At the bottom",
    collapse: "Collapse",
    expand: "Expand",
    addMenu: "Add a note",
    addMenuHint: "Add your notes",
    generalNote: "Note (no project)",
    onProject: "On a project",
    annotate: "Annotate",
    annotateTitle: "Write a comment on this point",
    placeholder: "Write\u2026",
    delete: "Delete",
    pointChanged: "The point this was written on has changed",
    otherAnnotations: "Other annotations",
    pageNotes: "Project notes",
    pageNotesTip:
      "Written on this page: they belong to no day or week, stay out of the summaries, and are never sent to the model",
    addPageNote: "Add a project note",
  },

  edits: {
    toggle: (n: number) => `Your edits \u00b7 ${n}`,
    toggleTitle: "Mark the lines you added or rewrote in the generated report",
    added: "Added by you",
    rewrote: (original: string) => `Rewritten by you. Generated text:\n${original}`,
    removed: (n: number) => (n === 1 ? "1 line removed" : `${n} lines removed`),
    removedTitle: (lines: string) => `Generated lines you removed:\n${lines}`,
    inside: "Your edits are in the collapsed details",
  },

  dayBoundary: {
    title: "New day starts at",
    tip: "Activity before this time counts towards the previous day, so late-night work stays in that evening's log. Changing it re-files all sessions; reports of affected days become out of date.",
    refiling: "Re-filing sessions\u2026",
    midnight: " (midnight)",
  },

  agents: {
    detected: "Detected",
    notFound: "Not found",
    dirExists: "Directory exists",
    dirMissing: "Directory not found",
    enabled: "Enabled",
    partial: "Partly supported",
    unsupported: "Version not supported",
    versions: "Versions found",
    stored: (n: number) => `${n} session${n === 1 ? "" : "s"} stored`,
    storedFrozen: (n: number) =>
      `Off · ${n} stored session${n === 1 ? " is" : "s are"} kept and shown, but no longer updated`,
    newerFormat: "This version's storage format is not supported yet; nothing is read",
    sessionsInNewerFormat: (n: number) =>
      `${n} session${n === 1 ? "" : "s"} saved in a newer storage format ${n === 1 ? "is" : "are"} not read yet`,
    untestedVersions: (v: string) => `Sessions from untested versions: ${v}`,
    unrecognizedLayout: "Session data has an unexpected structure; nothing is read",
    probeFailed: (msg: string) => `Could not check the local data: ${msg}`,
  },

  appIcon: {
    title: "App Icon",
  },

  updates: {
    title: "Check for updates automatically",
    tip: "When on, Lantai asks GitHub for new releases when it starts and once a day. It sends nothing but its version number, and it never downloads or installs anything: you decide when to update.",
    current: (version: string) => `This is Lantai ${version}.`,
    checkNow: "Check now",
    checking: "Checking\u2026",
    upToDate: (version: string) => `Lantai ${version} is the latest version.`,
    available: (latest: string) => `Lantai ${latest} is available.`,
    breaking: "It includes breaking changes: read the release notes and back up your data before updating.",
    viewRelease: "View release",
    skipVersion: "Skip this version",
    failed: (error: string) => `Could not check for updates: ${error}`,
  },

  cliTool: {
    title: "Command-line tool",
    tip: "Lantai includes the aj command, which runs the same engine in a terminal (aj --help lists what it can do). Installing it puts a link in ~/.local/bin; no administrator password is needed.",
    install: "Install",
    reinstall: "Reinstall",
    uninstall: "Uninstall",
    installed: (path: string) => `Installed at ${path}.`,
    missing: (path: string) => `Not installed. It will be linked at ${path}.`,
    stale: (path: string) => `${path} points to another copy of Lantai. Reinstall to link this one.`,
    other: (path: string) => `${path} already exists and was not made by Lantai, so it is left alone.`,
    notOnPath: "~/.local/bin is not on your PATH yet. Add this line to ~/.zshrc, then open a new terminal:",
    pathLine: 'export PATH="$HOME/.local/bin:$PATH"',
    translocated: "Move Lantai to the Applications folder and open it from there before installing the command-line tool.",
    notBundled: "This build does not include the command-line tool. Build it with cargo build --release -p aj-cli.",
  },

  backup: {
    exportTitle: "Back up data",
    exportTip:
      "Saves the database (sessions, projects and rules, prompts, reports with their versions, notes) and the settings to one zip file. API keys stay in the keychain and are not included.",
    export: "Export\u2026",
    exportPrompt: "Save the Lantai backup as:",
    exporting: "Writing the backup\u2026",
    exported: (path: string) => `Saved to ${path}`,
    restoreTitle: "Restore from a backup",
    restoreTip:
      "Replaces all data and settings with those in a backup. What is replaced is first kept in the backups folder of the data directory. API keys are left as they are.",
    restore: "Restore\u2026",
    restorePrompt: "Choose a Lantai backup:",
    confirm: (createdAt: string, version: string) =>
      `Backup of ${createdAt}, made by Lantai ${version}. Restoring replaces everything Lantai holds now.`,
    confirmButton: "Replace and restore",
    restoring: "Restoring\u2026",
    restored: (previous: string) =>
      `Restored. The replaced data is kept in ${previous}. Reloading\u2026`,
  },

  infoTip: {
    label: "More information",
  },

  copy: {
    copied: "Copied",
    failed: "Copy failed",
    asMarkdown: "Copy as Markdown",
  },

  slice: {
    select: (title: string) => `Select ${title}`,
    ignored: "Ignored",
    note: "Note",
    minutes: (n: number) => `${n}m`,
    turns: (n: number) => `${n}T`,
    dayOverride: "day override",
  },

  health: {
    loading: "Loading diagnostics...",
    title: "Diagnostics",
    refresh: "Refresh",
    sessions: "Sessions",
    turns: "Turns",
    irVersion: "IR version",
    dataDir: "Data directory",
    exportDir: "Default export",
    sourceStats: (files: number, errors: number, unknown: number) =>
      `${plural(files, "file", "files")} · ${plural(errors, "parse error", "parse errors")} · ${plural(unknown, "unknown record", "unknown records")}`,
    agentVersions: (first: string, last: string, count: number) =>
      `Agent versions: ${first} \u2026 ${last} (${count})`,
    latestError: (e: string) => `Latest error: ${e}`,
  },

  resize: {
    label: "Resize pane",
    title: "Drag to resize",
  },

  agentBadge: {
    source: (name: string) => `Source: ${name}`,
  },

  batch: {
    toolbar: "Selected sessions",
    selected: (n: number) => `${n} selected`,
    selectAll: "Select all",
    moveTitle: "Move the selected sessions to a project",
    moveTo: "Move to project\u2026",
    noProject: "No project",
    unassigned: "Unassigned",
    ignore: "Ignore",
    ignoreTitle: "Leave the selected sessions out of reports for their day",
    include: "Include",
    exit: "Exit selection",
    exitTitle: "Exit selection (Esc)",
  },

  dayBatch: {
    running: (done: number, total: number) => `Running ${done}/${total}`,
    cancelAll: "Cancel all",
    selected: (n: number) => `${plural(n, "day", "days")} selected`,
    exit: "Exit multi-select",
    exitTitle: "Exit multi-select (Esc)",
    confirmRegenerate: (n: number) => `Regenerate ${plural(n, "report", "reports")}?`,
    confirm: "Confirm",
    smartTitle: (missing: number, stale: number) => `${missing} to generate, ${stale} to update`,
    nothingToDo: "No missing or out-of-date reports in the selection",
    generateUpdate: "Generate / Update",
    options: "Batch generation options",
    updateStale: (n: number) => `Update out of date (${n})`,
    generateMissing: (n: number) => `Generate missing (${n})`,
    regenerateAll: (n: number) => `Regenerate all (${n})`,
    includeFinal: "Include final",
    final: "Final",
    finalTitle: "Mark the selected reports as final",
    draft: "Draft",
    draftTitle: "Reopen the selected reports as drafts",
  },

  dayBatchSummary: {
    noReports: "No existing reports to check.",
    allUpToDate: "All reports are up to date.",
    skippedNothing: (n: number) => `${n} skipped (nothing to run)`,
    nothingToRun: "Nothing to run.",
    generated: (n: number) => `${n} generated`,
    updated: (n: number) => `${n} updated`,
    skipped: (n: number) => `${n} skipped`,
    cancelled: (n: number) => `${n} cancelled`,
    failed: (n: number) => `${n} failed`,
    done: "Done.",
    marked: (n: number, status: "draft" | "final") =>
      `${n} marked ${status === "final" ? "final" : "draft"}`,
    skippedNoReport: (n: number) => `${n} skipped (no report)`,
  },

  preDigest: {
    waiting: (n: number) => `Waiting for ${plural(n, "live slice", "live slices")} to go quiet.`,
    allCached: "All active slices summarized.",
    running: (id: number) => `Summarizing S${id} in advance\u2026`,
    cached: (id: number) => `Summarized S${id}`,
    paused: (err: string) => `Summarizing in advance paused: ${err}`,
  },

  reportKinds: {
    daily: "Daily",
    weekly: "Weekly",
    monthly: "Monthly",
  } as Record<string, string>,

  prompts: {
    templates: "Templates",
    newName: "New prompt name",
    copyOf: (name: string, kind: string) => `Copy: ${name} (${kind})`,
    showAllLanguages: "Show all languages",
    default: "Default",
    builtIn: "Built-in",
    custom: "Custom",
    chars: (n: number) => `${n} chars`,
    selectHint: "Select a prompt to view or edit.",
    builtInTemplate: "Built-in template (editable)",
    customPrompt: "Custom prompt",
    setDefault: "Set as default",
    editView: "Edit",
    fullAssembled: "Full prompt",
    savePrompt: "Save prompt",
    otherLanguage: (promptLang: string, reportLang: string) =>
      `This prompt is tuned for ${promptLang}; reports are written in ${reportLang}. Length limits and wording may not fit. The report language is set in Settings > General.`,
  },

  projectList: {
    exists: (name: string) => `"${name}" already exists.`,
    empty: "No projects yet.",
    nameLabel: "Project name",
  },

  rollup: {
    summariseBy: "Summarize by",
    timeRange: "Time range",
    last7: "Last 7 days",
    last14: "Last 14 days",
    last30: "Last 30 days",
    allTime: "All time",
    oldestFirstTitle: "Oldest first. Click to show newest first.",
    newestFirstTitle: "Newest first. Click to show oldest first.",
    exportTitle: (project: string) => `Export ${project} roll-up`,
    unreported: (g: string, project: string) =>
      `Active ${unitOf(g)}s without a ${reportOf(g)} entry for ${project}:`,
    openWorkbench: (g: string, period: string) => `Open the ${unitOf(g)} ${period}`,
    noEntries: (g: string, project: string) => `No ${reportOf(g)} entries for ${project}`,
    pickToGenerate: (g: string) => `Pick a ${unitOf(g)} above to generate its report.`,
    noActivity: "No activity in this range.",
    status: (status: string) => `Report status: ${status === "final" ? "final" : "draft"}`,
    openReport: (g: string) => `Open ${reportOf(g)} ->`,
  },

  exportModal: {
    exported: (bytes: number, path: string) => `Exported ${bytes} bytes to ${path}`,
    closeLabel: "Close export dialog",
    folder: "Export folder",
    opening: "Opening...",
    chooseFolder: "Choose folder...",
    fileName: "File name",
    anchors: "Source anchors",
    keep: "Keep",
    footnotes: "Footnotes",
    hide: "Hide",
    conflict: "Export conflict:",
    modifiedOutside: "was modified outside Lantai.",
    previewExternal: "Preview external file",
    overwrite: "Overwrite",
    imported: "Imported the external file as a new version.",
    importAsVersion: "Import as version",
    done: "Done",
    exporting: "Exporting...",
    exportMarkdown: "Export Markdown",
  },

  drawer: {
    user: "User:",
    noPrompt: "(no prompt; continued from previous context)",
    result: "Result:",
    details: (expanded: boolean, tools: number, earlier: number) =>
      `${expanded ? "Hide" : "Show"} details (${plural(tools, "tool", "tools")}${earlier > 0 ? `, ${plural(earlier, "earlier reply", "earlier replies")}` : ""})`,
    error: "error",
    resize: "Resize drawer",
    loading: "Loading...",
    chars: (n: number) => `${n} chars`,
    loadingDetails: "Loading slice details...",
    directory: "Directory:",
    branch: "Branch:",
    remote: "Remote:",
    projectOverride: "Slice project override:",
    followSession: "Follow session",
    ignoreState: "Ignore state:",
    inheritSession: "Inherit session",
    include: "Include",
    ignore: "Ignore",
    notePlaceholder: "Note for the report (takes priority)",
    saveNote: "Save note",
    modifiedFiles: (n: number) => `Modified files (${n})`,
    digest: "Slice digest",
    summarizing: "Summarizing...",
    refreshDigest: "Refresh digest",
    generateDigest: "Generate digest",
    noDigest: "No digest yet.",
    goal: "Goal:",
    keyChanges: "Key changes:",
    decisions: "Decisions & pitfalls:",
    outcome: (outcome: string) => `Outcome: ${outcome || "unspecified"}`,
    open: (items: string) => ` · Open: ${items}`,
    turns: (n: number) => `Turns (${n})`,
  },

  projectsSettings: {
    addRule: "Add rule",
    addRuleTip: "Match sessions by directory prefix, git remote, or regex. Automatic rules assign directly; the others only suggest.",
    noProjects: "No projects yet",
    ruleTypes: {
      path_prefix: "Directory prefix",
      git_remote: "Git remote",
      regex: "Regex",
    } as Record<string, string>,
    fields: {
      any: "Field: any",
      title: "Field: title",
      first_prompt: "Field: first prompt",
      cwd: "Field: directory",
    } as Record<string, string>,
    regexPlaceholder: "case-insensitive regex pattern",
    autoOnMatch: "Assign automatically on match",
    activeRules: (n: number) => `Active rules (${n})`,
    noRules: "No assignment rules yet.",
    auto: "Auto",
    createProject: "Create project",
    name: "Name",
    namePlaceholder: "e.g. Lantai",
    description: "Description",
    descriptionTip: "Optional. Sent with unassigned sessions so the model can suggest this project for them.",
    optional: "Optional",
    dirPrefix: "Directory prefix (optional)",
    gitRemote: "Git remote (optional)",
    autoStarter: "Assign automatically with the starter rules",
    configured: (n: number) => `Projects (${n})`,
    noneCreated: "No projects yet.",
    projectName: "Project name",
    descriptionPlaceholder: "Description",
    counts: (slices: number, suggested: number, rules: number) =>
      `${plural(slices, "slice", "slices")} · ${suggested} suggested · ${plural(rules, "rule", "rules")}`,
    edit: "Edit",
    clusters: (n: number) => `Unassigned session groups (${n})`,
    sessions: (n: number) => plural(n, "session", "sessions"),
    remote: (r: string) => `Remote: ${r}`,
    useTemplate: "Use as template",
  },

  sliceList: {
    noProject: "No project",
    unassigned: "Unassigned",
    all: (n: number) => `All (${n})`,
    unassignedFilter: (n: number) => `Unassigned (${n})`,
    ignoredFilter: (n: number) => `Ignored (${n})`,
    acceptSuggestions: (n: number) => `Accept suggestions (${n})`,
    suggestionsBanner: (n: number) =>
      `${plural(n, "session has", "sessions have")} a suggested project (shown on the row).`,
    newProjectFor: (title: string) => `New project for \u201c${title}\u201d`,
    projectNamePlaceholder: "Project name (e.g. Atlas)",
    descriptionPlaceholder: "Description (helps the model suggest sessions)",
    noRuleBefore: "No path rule for ",
    noRuleMiddle: " yet. Save one for ",
    noRuleAfter: "?",
    suggestOnly: "Suggest only",
    autoAssign: "Assign automatically",
    dismiss: "Dismiss",
    include: "Include",
    empty: (date: string) => `No matching session slices for ${date}.`,
    ignoreTitle: "Leave this session out of reports for this day",
    ignore: "Ignore",
    suggestion: (source: string | null, reason: string | null) =>
      `${source === "llm" ? "model" : (source ?? "rule")}${reason ? `: ${reason}` : ""}`,
    assignTitle: "Assign project",
    newProject: "New project\u2026",
    followSession: "Follow session",
  },

  about: {
    version: (v: string) => `Version ${v}`,
    sponsor: "Sponsor Lantai on GitHub",
    sponsorAfdian: "Sponsor Lantai on Afdian",
  },

  tokens: {
    title: "Token usage",
    parts: {
      outputTokens: "Output",
      inputTokens: "Input",
      cacheWriteTokens: "Cache write",
      cacheReadTokens: "Cache read",
    },
    empty:
      "No token usage recorded in this range. Usage is read from Claude Code, Codex and Antigravity session files; rescan after upgrading to pick up older sessions.",
    calls: (n: string) => `${n} model calls \u00b7 main sessions only`,
    total: "Total",
    cacheWriteSub: (n: string) => `+ ${n} cache write`,
    cacheHitSub: (pct: string) => `${pct} of input hits cache`,
    composition: "Token composition",
    perBucket: (week: boolean): string => (week ? "Per week" : "Per day"),
    includeCacheReads: "Include cache reads",
    tokensUnit: " tokens",
    byModel: "By model",
    unknownModel: "Unknown model",
    modelSub: (agent: string, calls: string, out: string) =>
      `${agent} \u00b7 ${calls} calls \u00b7 out ${out}`,
  },

  lantai: {
    title: "Lantai's stats",
    checking: "Checking\u2026",
    couldNotLoad: (e: string) => `Could not load: ${e}`,
    status: {
      none: "Not generated",
      draft: "Draft",
      final: "Final",
      ignored: "All sessions ignored",
    },
    digest: { fresh: "Reusable", behind: "Behind", missing: "Missing" },
    versions: (n: number) => `${n} versions`,
    editedByHand: "edited by hand",
    outOfDate: "out of date",
    outOfDateIn: (projects: string) => `out of date: ${projects}`,
    reportFigure: (finals: number, drafts: number, missing: number, edited: number) =>
      `${finals} final \u00b7 ${drafts} draft \u00b7 ${missing} not generated` +
      (edited > 0 ? ` \u00b7 ${edited} edited by hand` : ""),
    staleCount: (n: number) => `${n} out of date`,
    daily: "Daily reports",
    weekly: "Weekly reports",
    monthly: "Monthly reports",
    split: (n: string) => `${n} split into parts`,
    incremental: (n: string) => `${n} incremental updates`,
    briefs: (n: string) => `${n} project briefs`,
    digestCache: "Digest cache",
    months: "Months",
    weeks: "Weeks",
    days: "Days",
    digests: "Digests",
    legendOutOfDate: "Out of date",
  },

  stats: {
    title: "Stats",
    upTo: (date: string) => `Up to ${date}`,
    refreshing: "Refreshing\u2026",
    includeIgnoredTitle: "Include ignored sessions and slices",
    includeIgnored: "Include ignored",
    rangeLabel: "Stats time range",
    ranges: { "7d": "7D", "30d": "30D", "90d": "90D", all: "All" },
    loading: "Loading stats\u2026",
    failed: "Unable to load statistics.",
    empty: "No activity recorded in this window. Run Scan to load sessions or switch to a wider range.",
    activeTime: "Active time",
    perActiveDay: (d: string) => `${d} / active day`,
    noActiveDays: "No active days",
    activeDays: "Active days",
    streak: (cur: number, best: number) => `Streak ${cur}d \u00b7 Best ${best}d`,
    bestStreak: (best: number) => `Best ${best}d streak`,
    sessions: "Sessions",
    turnsPerSession: (turns: string, avg: string) => `${turns} turns \u00b7 ${avg} / session`,
    filesChanged: "Files changed",
    activityTrend: "Activity trend",
    granularity: (day: boolean): string => (day ? "Daily" : "Weekly"),
    peak: (label: string, d: string) => `Peak ${label}: ${d}`,
    other: "Other",
    unassigned: "Unassigned",
    sessionCount: (n: number) => plural(n, "session", "sessions"),
    noActivity: "No activity",
    moreProjects: (n: number) => `+${n} more projects`,
    trendAria: "Activity trend stacked by project",
    byProject: "By project",
    open: (name: string) => `Open ${name}`,
    projectSub: (days: number, sessions: number, files: number) =>
      `${days}d \u00b7 ${plural(sessions, "session", "sessions")} \u00b7 ${plural(files, "file", "files")}`,
    showFewer: "Show fewer",
    showAll: (n: number) => `Show all (${n})`,
    byAgent: "By agent",
    agentSub: (sessions: number, turns: string, files: number) =>
      `${plural(sessions, "session", "sessions")} \u00b7 ${turns} turns \u00b7 ${plural(files, "file", "files")}`,
    toolProfile: "Tool profile",
    tools: {
      code: "Edit & write",
      shell: "Shell commands",
      read: "File reads",
      web: "Web search & fetch",
      other: "Subagent, MCP & other",
    } as Record<string, string>,
    failedCommands: (n: number) => `${n} failed commands`,
    failedPct: (pct: number) => `${pct}% failed`,
    workRhythm: "Work rhythm",
    localTime: "Local time",
    night: "Night 22\u201305",
    rhythmPeak: (hour: string, pct: number) => `Peak ${hour}:00 \u00b7 ${pct}% after 22:00`,
    rhythmAria: "24-hour work rhythm",
  },

  search: {
    title: "Search",
    placeholder: "Search reports and sessions",
    hint: 'Every word must appear; use "double quotes" for a phrase. Matching is case-insensitive.',
    layers: { all: "All", reports: "Reports", sessions: "Sessions" } as Record<string, string>,
    layerLabel: "Search in",
    allProjects: "All projects",
    allAgents: "All agents",
    since: "From",
    until: "To",
    includeIgnored: "Ignored",
    includeIgnoredTitle: "Also search sessions that are ignored",
    toolOutput: "Tool output",
    toolOutputTitle: "Also search the output of tool calls (noisy)",
    agentReportsNote: "The agent filter applies to sessions only.",
    searching: "Searching…",
    empty: "Type to search your reports and sessions.",
    noResults: "Nothing found.",
    reports: (n: number) => `Reports · ${plural(n, "section", "sections")}`,
    sessions: (slices: number, turns: number) =>
      `Sessions · ${plural(slices, "slice", "slices")}, ${plural(turns, "turn", "turns")}`,
    reportsTruncated: "Showing the newest 100 sections; narrow the search to see older ones.",
    sessionsTruncated: "Stopped after 400 matching turns; older matches are not shown. Narrow the search.",
    header: "Header",
    note: "Note",
    untitled: "(untitled)",
    unassigned: "Unassigned",
    ignored: "Ignored",
    fields: {
      prompt: "Prompt",
      reply: "Reply",
      tool: "Tool",
      tool_output: "Output",
      title: "Title",
      note: "Note",
    } as Record<string, string>,
    citedBy: "Cited by",
    openReport: "Open this report",
    openTurn: "Open this turn in the session drawer",
    elapsed: (ms: number) => `${ms} ms`,
  },

  projectArchive: {
    archive: "Archive project",
    restore: "Restore project",
    archived: "Archived",
    group: (n: number) => `Archived (${n})`,
    banner:
      "This project is archived. Its sessions, reports and notes are kept; its rules are paused and it is not offered for new assignments.",
    rulesPaused: "Paused while the project is archived",
    deleteTitle: (name: string) => `Delete \u201c${name}\u201d?`,
    deleteLead: "This cannot be undone.",
    deleteSessions: (n: number) =>
      `${plural(n, "session", "sessions")} assigned to it will become unassigned`,
    deleteReports: (n: number) =>
      `${plural(n, "project report", "project reports")} and all their versions will be deleted`,
    deleteRules: (n: number) => `${plural(n, "assignment rule", "assignment rules")} will be deleted`,
    deleteNotes: (n: number) =>
      `${plural(n, "project note", "project notes")} will become notes of their day, led by the project name`,
    deleteKept: "Daily, weekly and monthly reports keep their sections for this project.",
    archiveHint: "To put the project away and keep everything, archive it instead.",
    archiveInstead: "Archive instead",
    confirmDelete: "Delete project",
  },

  timeline: {
    acceptAll: (n: number) => `Accept all (${n})`,
    showAll: (n: number) => `Show all ${n}`,
    showFewer: "Show fewer",
    editDetails: "Edit project details\u2026",
    toggleIgnored: (show: boolean, n: number) => `${show ? "Hide" : "Show"} ignored (${n})`,
    deleteProject: "Delete project\u2026",
    emptyTitle: "No sessions in this range",
    empty: (name: string) => `No sessions assigned to ${name} in the selected time range.`,
    ignoreTitle: "Leave this session out of reports for its day",
    suggested: (n: number) => `Suggested sessions (${n})`,
    accept: "Accept",
    addRuleFor: (name: string) => `Add rule for ${name}`,
    noRulesFor: (name: string) => `No rules for ${name} yet.`,
  },

  report: {
    widthHandle: "Drag to change the report width · double-click to reset",
    newSlices: (n: number) => `${n} new ${n === 1 ? "slice" : "slices"}`,
    changed: (n: number) => `${n} changed`,
    removed: (n: number) => `${n} removed`,
    estimateMonthly: (weeks: number, slices: number) =>
      `${weeks} weekly ${weeks === 1 ? "report" : "reports"} \u00b7 ${slices} slices`,
    estimate: (slices: number, cached: number, pending: number) =>
      `${slices} slices (${cached} cached, ${pending} pending)`,
    chars: (n: string) => ` \u00b7 ~${n} chars`,
    unreportedWeeks: (month: string) => `Active weeks in ${month} without a weekly report yet:`,
    openWeek: (w: string) => `Open weekly workbench for ${w}`,
    writingStage: "Writing report (stage 2)",
    digestingStage: "Digesting sessions (stage 1)",
    statusTitle: (status: string, estimate: string) =>
      `Report status: ${status}${estimate ? ` \u00b7 ${estimate}` : ""}`,
    mode: "Reading or editing",
    preview: "Preview",
    edit: "Edit",
    writing: "Writing\u2026",
    digesting: "Digesting\u2026",
    toggleStatus: (title: string, final: boolean) =>
      `${title}. Click to ${final ? "reopen as draft" : "mark final"}.`,
    draft: "Draft",
    final: "Final",
    updateTitle: (projects: string) =>
      `Rewrite only ${projects}; other sections, including your edits, stay as they are`,
    update: "Update",
    regenerate: "Regenerate",
    generate: "Generate",
    options: "Options",
    prompt: "Prompt",
    model: "Model",
    defaultSuffix: " (default)",
    regenerateWhole: "Regenerate whole report",
    versionHistory: (n: number) => `Version history (${n})`,
    exportMarkdown: "Export Markdown\u2026",
    exportTitle: (kind: string): string =>
      kind === "monthly"
        ? "Export monthly report"
        : kind === "weekly"
          ? "Export weekly report"
          : "Export daily report",
    noReport: (kind: string, period: string): string =>
      `No ${kind === "monthly" ? "monthly" : kind === "weekly" ? "weekly" : "daily"} report for ${period} yet`,
    weeklyFirst: "Generate the month's weekly reports first, then generate the monthly report.",
    reviewThen: (kind: string): string =>
      `Review the ${kind === "monthly" ? "month" : kind === "weekly" ? "week" : "day"}'s sessions, then generate.`,
    generating: "Generating\u2026",
    generateKind: (kind: string): string =>
      kind === "monthly"
        ? "Generate monthly report"
        : kind === "weekly"
          ? "Generate weekly report"
          : "Generate daily report",
    reviewSessions: "Review sessions",
    outOfDate: (summary: string, projects: string) =>
      `Out of date: ${summary} since this version \u00b7 affects ${projects}`,
    updateSections: (n: number): string => (n === 1 ? "Update section" : "Update sections"),
    openProjectReport: "Open project report \u2192",
    saveAsNewVersion: "Save as new version",
    back: "\u2190 Back to report",
    left: "Left:",
    right: "Right:",
    origins: { generated: "generated", user_edit: "edited" } as Record<string, string>,
    manual: "manual",
    setCurrent: "Set current",
    promptSnapshot: "Prompt snapshot",
  },

  anchors: {
    openSlice: (slice: number) => `Open slice S${slice}`,
    openTurn: (slice: number, turn: number) => `Open slice S${slice} at turn T${turn}`,
    collapse: "Collapse details",
    expand: "Expand details",
  },

  settings: {
    title: "Settings",
    categories: "Settings categories",
    close: "Close settings",
    tabs: {
      appearance: { label: "Appearance", title: "Appearance & layout" },
      language: { label: "Language", title: "Language" },
      prompts: { label: "Prompts", title: "Report prompts" },
      projects: { label: "Projects", title: "Projects" },
      rules: { label: "Rules", title: "Assignment rules" },
      models: { label: "Models", title: "Models & providers" },
      agents: { label: "Agents", title: "Agent sources" },
      system: { label: "System", title: "System" },
      about: { label: "About", title: "About" },
    },
    colorTheme: "Color theme",
    light: "Light",
    dark: "Dark (low glare)",
    themeSubtitles: {
      "github-light": "Crisp neutral daytime",
      "solarized-light": "Warm soft paper",
      "catppuccin-latte": "Gentle cool latte",
      "one-dark": "Balanced slate charcoal",
      "tokyo-night": "Low-glare indigo dusk",
      "catppuccin-mocha": "Soft matte espresso",
    },
    spacingLayout: "Spacing & layout",
    density: "Spacing density",
    densities: {
      compact: { label: "Compact", hint: "Tight padding, fits more rows" },
      default: { label: "Default", hint: "Balanced spacing" },
      comfortable: { label: "Comfortable", hint: "Generous breathing room" },
    },
    paneProportions: "Pane proportions",
    drawerWidth: (px: number) => `Drawer ${px}px`,
    resetDone: "Reset done",
    resetLayout: "Reset spacing & layout",
    reportWidth: "Report width",
    reportWidthHint: (px: number, def: number) =>
      `${px}px (default ${def}px). You can also drag either edge of a report while reading it.`,
    resetReportWidth: "Reset width",
    preDigest: "Background pre-digest",
    preDigestTip:
      "Summarize pending slices in the background while the Daily view is idle, so reports generate faster. Choose which agents' sessions to pre-digest.",
    noAgentEnabled: "No agent enabled",
  },

  languageSettings: {
    interface: "Interface language",
    interfaceTip: "The language of menus, buttons and messages.",
    auto: (name: string) => `System (${name})`,
    report: "Report language",
    reportTip:
      "The language reports are written in, and the prompts offered for it. Reports already written keep their language; Update rewrites a report fully when the language changes.",
    tuned: "Tuned prompts",
    borrowed: "Uses the English prompts",
    changed: (name: string) => `Reports will now be written in ${name}.`,
  },

  models: {
    success: "Success",
    failed: "Failed",
    defaultInUse: (alias: string) =>
      `\u201c${alias}\u201d is the default model; make another model the default before disabling it.`,
    loading: "Loading model settings\u2026",
    dismiss: "Dismiss",
    defaultModel: "Default model",
    defaultModelTip:
      "Used for slice digests and reports unless another model is picked in the report toolbar.",
    models: "Models",
    modelsTip:
      "Your names for a provider's model plus its options. Enable models quickly with Browse models under Providers.",
    addModel: "Add model",
    alias: "Alias",
    aliasPlaceholder: "e.g. flash",
    provider: "Provider",
    kindName: (kind: string): string =>
      kind === "claude_cli" ? "Claude CLI" : "OpenAI-compatible",
    modelId: "Model ID",
    optionalSuffix: " (optional)",
    cliDefault: "CLI default",
    modelIdPlaceholder: "e.g. gpt-5-mini",
    fetching: "Fetching\u2026",
    fetchList: "Fetch list",
    effort: "Reasoning effort (optional)",
    serverDefault: "server default",
    filter: (n: number) => `Filter ${n} models`,
    noMatches: "No matching models.",
    saveModel: "Save model",
    default: "Default",
    defaultModelId: "default",
    testing: "Testing\u2026",
    test: "Test",
    setDefault: "Set default",
    deleteModel: (alias: string) => `Delete model alias \u201c${alias}\u201d?`,
    providers: "Providers",
    providersTipBefore: "Connections to model services. API keys are stored in the ",
    providersTipAfter: ", never in ",
    config: "Config: ",
    addProvider: "Add provider",
    name: "Name",
    namePlaceholder: "e.g. openai",
    type: "Type",
    openaiType: "OpenAI-compatible API",
    cliType: "Claude Code CLI",
    baseUrl: "Base URL",
    preset: "Preset",
    fillPreset: "Fill from a preset\u2026",
    presetGroups: {
      vendor: "Model makers",
      aggregator: "Aggregators",
      local: "Local",
    },
    presetRegion: (name: string, region: "global" | "china") =>
      `${name} (${region === "china" ? "China" : "Global"})`,
    apiKey: "API key",
    stored: "Stored",
    keepKey: "Leave blank to keep the stored key",
    noAuth: "Leave blank for endpoints without auth",
    timeout: "Timeout in seconds (optional)",
    command: "Command (optional)",
    connected: (n: number) => `Connected \u00b7 ${n} models`,
    fetchModels: "Fetch models",
    removeKey: "Remove stored key",
    saveProvider: "Save provider",
    keyStored: "Key stored",
    noKey: "No key",
    hideModels: "Hide models",
    browseModels: "Browse models",
    usedBy: "Delete or repoint the models that use it first",
    deleteProvider: (name: string) => `Delete provider \u201c${name}\u201d and its stored key?`,
    fetchingFrom: (name: string) => `Fetching models from ${name}\u2026`,
    browseTip: "Click a model to enable it; click again to disable.",
    noModel: "No model yet. Find an agent CLI on this Mac below, or add a provider.",
    localTools: "Agent CLIs on this Mac",
    localToolsTip:
      "Lantai can write with an agent CLI you already use. Finding only runs each tool's --version, so it checks neither sign-in nor quota; Test does. A CLI is used only if it can run once with every tool turned off and without leaving a session behind.",
    localToolsIntro:
      "Look for Claude Code, Codex CLI, Antigravity and OpenCode on this Mac. Nothing is added until you choose.",
    findTools: "Find tools",
    finding: "Looking\u2026",
    findAgain: "Find again",
    noToolsFound: "No known agent CLI was found on PATH.",
    addTool: "Add",
    addedAs: (alias: string) => `Added as \u201c${alias}\u201d`,
    cannotGenerate: "Can't generate yet",
    gaps: {
      tools: "its tools cannot all be turned off",
      sessions: "every run leaves a session behind",
    } as Record<string, string>,
    versionUnknown: "version unknown",
  },

  failures: {
    no_model: {
      title: "No model is set up",
      next: "Find an agent CLI on this Mac or add a provider in Settings \u2192 Models.",
    },
    command_not_found: {
      title: "The command could not be found",
      next: "It may have moved or been uninstalled. Find tools again in Settings \u2192 Models, or fix the provider's command.",
    },
    not_signed_in: {
      title: "Not signed in, or the key was rejected",
      next: "For Claude Code, run claude in Terminal and sign in. For an API provider, check its key in Settings \u2192 Models.",
    },
    quota: {
      title: "Usage limit or quota reached",
      next: "Wait for the limit to reset, or pick another model in the report toolbar.",
    },
    details: "Details",
    openModels: "Model settings",
  },

  workspace: {
    viewLabel: (mode: string): string =>
      mode === "monthly"
        ? "Monthly workspace view"
        : mode === "weekly"
          ? "Weekly workspace view"
          : "Daily workspace view",
    projectViewLabel: "Project workspace view",
    report: "Report",
    sessions: (n: number) => `Sessions (${n})`,
    rules: (n: number) => `Assignment rules (${n})\u2026`,
    selectProject: "Select a project from the sidebar.",
    dismiss: "Dismiss",
  },

  scan: {
    result: (files: number, turns: number, suggestions: number) =>
      `${files} parsed \u00b7 +${turns} turns \u00b7 ${suggestions} suggestions`,
  },
};

export type Messages = typeof en;

/** Texts a language may leave out; functions are replaced whole. */
export type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends (...args: never[]) => unknown
    ? T[K]
    : T[K] extends object
      ? DeepPartial<T[K]>
      : T[K];
};
