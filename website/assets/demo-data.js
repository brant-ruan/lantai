// Made-up data for the interactive demo on the homepage. Nothing here comes from real sessions:
// four fictional projects, a schedule of which work happened on which day, and hand-written
// monthly summaries. demo.js derives sessions, turns and reports from it.
window.LANTAI_DEMO_DATA = {
  today: "2026-11-19",

  agents: {
    claude: "Claude Code",
    codex: "Codex CLI",
    antigravity: "Antigravity",
    dsh: "DeepSeek Harness",
    workbuddy: "WorkBuddy",
  },

  projects: [
    {
      key: "B",
      name: "Billing Service",
      desc: "Payments backend: invoices, retries, refunds",
      dir: "~/code/billing-service",
      remote: "git.example.com/payments/billing-service",
      agents: ["claude", "claude", "codex"],
      files: ["internal/invoice/retry.go", "internal/invoice/queue.go", "internal/refund/service.go", "internal/webhook/handler.go"],
    },
    {
      key: "D",
      name: "Docs Site",
      desc: "Public developer documentation",
      dir: "~/code/docs-site",
      remote: "git.example.com/payments/docs",
      agents: ["antigravity", "workbuddy"],
      files: ["content/guides/quick-start.mdx", "components/CodeTabs.tsx", "scripts/check-links.ts", "content/reference/api.mdx"],
    },
    {
      key: "C",
      name: "CLI Toolkit",
      desc: "The `tidy` command-line client",
      dir: "~/code/tidy-cli",
      remote: "git.example.com/tools/tidy",
      agents: ["codex", "dsh"],
      files: ["src/cli/args.rs", "src/commands/list.rs", "src/output/json.rs", ".github/workflows/release.yml"],
    },
    {
      key: "F",
      name: "Field App",
      desc: "iOS app for taking notes on site visits",
      dir: "~/code/field-app",
      remote: "git.example.com/mobile/field-app",
      agents: ["claude", "antigravity"],
      files: ["FieldApp/Sync/SyncQueue.swift", "FieldApp/Map/MapView.swift", "FieldApp/Notes/NoteEditor.swift", "FieldAppTests/SyncTests.swift"],
    },
  ],

  // Work items per project: session title (s), the opening prompt (q), the key result (p)
  // and optional details (d). Each scheduled item becomes one session.
  items: {
    B: {
      1: { s: "Trace duplicate invoice charges", q: "Pro customers were charged twice last night. Find out why.", p: "Traced the duplicate charges to two webhook workers racing on one invoice", d: ["Both workers passed the unpaid check before either wrote the payment row", "Reproduced with a two-worker integration test (fails 3 runs in 10)"] },
      2: { s: "Idempotent retry queue", q: "Move invoice retries onto a job queue keyed by invoice id so duplicates become no-ops.", p: "Moved invoice retries onto an idempotent job queue", d: ["Jobs keyed by invoice id, so duplicate webhooks are now no-ops", "Backoff capped at 6 attempts; failures land in a review list"] },
      3: { s: "Backfill double-charged invoices", q: "Write a script that finds and refunds invoices charged twice since Monday.", p: "Found 37 double-charged invoices and prepared a refund script for review", d: ["Dry run only; totals match the payment provider's export"] },
      4: { s: "Refund flow audit", q: "Do refunds go through the new queue too?", p: "Open question: refunds still bypass the queue and can race with retries", d: ["Left a failing test that documents the race"] },
      5: { s: "Route refunds through the queue", q: "Route refunds through the same job queue and make the failing test pass.", p: "Routed refunds through the job queue; the race test now passes", d: ["Refund jobs share the invoice lock key", "Added a metric for jobs waiting on the lock"] },
      6: { s: "JPY rounding bug", q: "Totals on JPY invoices are off by one yen. Why?", p: "Fixed rounding for zero-decimal currencies (JPY, KRW) in invoice totals", d: ["Amounts were rounded to cents before converting to minor units"] },
      7: { s: "Tax line items on invoices", q: "Add separate tax line items to the invoice PDF.", p: "Invoice PDFs now show tax as separate line items per jurisdiction", d: ["Snapshot tests updated for three sample invoices"] },
      8: { s: "Webhook signature check", q: "Review how we verify the provider's webhook signatures.", p: "Tightened webhook verification: events older than 5 minutes are rejected", d: ["Replaying an old event now returns 400 instead of re-processing it"] },
      9: { s: "Slow invoice list", q: "The invoice list takes 4 seconds for large accounts. Profile it.", p: "Cut the invoice list query from 4.1 s to 180 ms with a composite index", d: ["Index on (account_id, created_at desc); query plan attached to the PR"] },
      10: { s: "Dunning schedule", q: "Draft the dunning schedule: emails on day 1, 3 and 7 after a failed payment.", p: "Added a dunning schedule (day 1, 3, 7) for failed payments", d: ["Emails reuse existing templates; copy still needs review"] },
      11: { s: "Proration on upgrades", q: "Upgrading mid-cycle charges the full price. Implement proration.", p: "Implemented proration for mid-cycle plan upgrades", d: ["Credit for unused time goes onto the next invoice", "Downgrades take effect at the end of the period"] },
      12: { s: "Flaky payment tests", q: "The payment integration tests fail randomly in CI. Look into it.", p: "Fixed flaky payment tests caused by a clock shared between test cases", d: ["Each test now gets its own fake clock"] },
      13: { s: "Provider SDK upgrade", q: "Upgrade the payment provider SDK to v12 and fix the breaking changes.", p: "Upgraded the payment provider SDK to v12", d: ["Two renamed fields; no behaviour change in tests"] },
      14: { s: "Usage-based billing design", q: "Sketch how metered usage could be billed at the end of each period.", p: "Drafted a design for usage-based billing built on daily usage snapshots", d: ["Open: how to bill usage events that arrive late"] },
      15: { s: "Coupon stacking", q: "Can customers stack two coupons? They shouldn't be able to.", p: "Blocked coupon stacking at checkout: one coupon per subscription", d: [] },
      16: { s: "Monthly invoice export", q: "Finance wants a CSV export of invoices for each month.", p: "Added a monthly CSV export of invoices for finance", d: ["Rows are streamed, so large months don't load into memory"] },
      17: { s: "Failed retries admin page", q: "Add a small admin page that lists failed retries.", p: "Built an admin page of failed retries with a manual Retry now button", d: ["Support staff can view it; only admins can retry"] },
      18: { s: "Double-charge postmortem", q: "Help me write up the double-charge incident for the postmortem.", p: "Wrote the double-charge postmortem: timeline, root cause and follow-ups", d: ["Three follow-ups filed: refund race, alerting, load test"] },
    },
    D: {
      1: { s: "Quick-start rewrite", q: "Rewrite the quick-start around the new config format.", p: "Rewrote the quick-start around the new config format", d: ["Every snippet now runs as-is, checked by the docs test runner"] },
      2: { s: "Dark-mode screenshots", q: "Add dark-mode screenshots to the API reference.", p: "Added dark-mode screenshots to the API reference", d: [] },
      3: { s: "Broken link checker", q: "Add a CI step that fails on broken internal links.", p: "Added a CI link checker and fixed 23 broken internal links", d: ["External links are only reported, to avoid flaky builds"] },
      4: { s: "Search returns nothing", q: "Search returns nothing for 'webhook'. Check the index setup.", p: "Fixed search indexing for pages under /guides", d: ["The indexer skipped folders that contain an index.mdx"] },
      5: { s: "Versioned docs", q: "Set up versioned docs for v2 and v3.", p: "Set up a version switcher for the v2 and v3 docs", d: ["v2 pages carry a banner pointing to the migration guide"] },
      6: { s: "v2 to v3 migration guide", q: "Draft the v2 to v3 migration guide from the changelog.", p: "Drafted the v2 → v3 migration guide", d: ["Covers config, auth and the renamed CLI flags"] },
      7: { s: "Code tabs component", q: "Make a code-tabs component so examples can switch between Go, Python and curl.", p: "Built a code-tabs component for multi-language examples", d: ["The chosen language is remembered across pages"] },
      8: { s: "Slow docs on mobile", q: "Docs pages feel slow on phones. Check the bundle.", p: "Halved the docs JavaScript by lazy-loading the API playground", d: ["Lighthouse mobile performance went from 61 to 92"] },
      9: { s: "Changelog page", q: "Generate the changelog page from the release notes.", p: "The changelog page now builds from release notes automatically", d: [] },
      10: { s: "Billing glossary", q: "Write a glossary of billing terms for the docs.", p: "Added a glossary of 18 billing terms, linked from the guides", d: [] },
    },
    C: {
      1: { s: "zsh and fish completion", q: "Generate shell completion for zsh and fish from the argument parser.", p: "Shell completion for zsh and fish, generated from the parser", d: ["Generated at build time, so completions never drift from the flags"] },
      2: { s: "Config file discovery", q: "Look for the config file in the project root, then in the XDG config dir.", p: "Config discovery now checks the project root, then the XDG config directory", d: [] },
      3: { s: "JSON output", q: "Add a --json flag to every list command.", p: "Added --json output to all list commands", d: ["Field names match the API, so scripts can reuse their types"] },
      4: { s: "Upload progress", q: "Long uploads show nothing. Add a progress bar.", p: "Added upload progress bars, hidden when output isn't a terminal", d: [] },
      5: { s: "Windows paths with spaces", q: "Paths with spaces break on Windows. Fix it.", p: "Fixed paths containing spaces on Windows", d: ["Uses the platform path API instead of splitting strings"] },
      6: { s: "Release automation", q: "Build binaries for macOS, Linux and Windows whenever we push a tag.", p: "Automated releases: tagged builds publish binaries for three platforms", d: ["Checksums and the Homebrew formula come from the same job"] },
      7: { s: "Readable API errors", q: "API errors are printed as raw JSON. Make them readable.", p: "API errors now print a one-line summary with a hint", d: ["--verbose still shows the raw response"] },
      8: { s: "Update check", q: "Tell users when a newer version exists, at most once a day.", p: "Added a once-a-day update check that never blocks a command", d: [] },
      9: { s: "Help snapshot tests", q: "Add snapshot tests for the help output.", p: "Added snapshot tests for the help text of all 14 commands", d: [] },
      10: { s: "Remove telemetry call", q: "Remove the leftover telemetry call and document what the CLI sends.", p: "Removed a leftover telemetry call; the CLI sends nothing by default", d: [] },
    },
    F: {
      1: { s: "Offline sync model", q: "How should notes sync when the device is offline for days?", p: "Chose per-field last-writer-wins for offline sync, with a conflict log", d: ["Whole-record merges lost edits in testing"] },
      2: { s: "Sync queue on SQLite", q: "Implement the local sync queue on top of SQLite.", p: "Implemented the local sync queue on SQLite", d: ["The queue survives restarts and retries with exponential backoff"] },
      3: { s: "Photo attachments", q: "Let users attach photos to a note, compressed before upload.", p: "Added photo attachments, compressed to under 1 MB before upload", d: [] },
      4: { s: "Map pin clustering", q: "The map is unreadable with 500 pins. Cluster them.", p: "Clustered map pins; 500 notes now render smoothly", d: [] },
      5: { s: "Dark mode pass", q: "Go through every screen and fix the dark-mode colors.", p: "Fixed dark-mode colors on 9 screens", d: ["Hard-coded colors moved into the asset catalog"] },
      6: { s: "Launch crash on iOS 17", q: "Crash reports show a launch crash on iOS 17 only. Investigate.", p: "Fixed a launch crash on iOS 17 caused by an unguarded API call", d: ["Added an availability check and a launch UI test on iOS 17"] },
      7: { s: "VoiceOver labels", q: "VoiceOver just says 'button' for most icons. Add labels.", p: "Added VoiceOver labels to every icon button", d: [] },
      8: { s: "TestFlight 0.9", q: "Prepare a TestFlight build with release notes.", p: "Shipped TestFlight build 0.9 (42) with release notes", d: [] },
      9: { s: "Conflict log screen", q: "Show the sync conflict log in Settings.", p: "Added the sync conflict log under Settings", d: [] },
      10: { s: "Background battery use", q: "Location updates drain the battery. Can we do fewer?", p: "Cut background location updates; idle battery use is down about 40%", d: ["Significant-change monitoring instead of continuous updates"] },
    },
  },

  // Sessions not yet in any project. `suggest` is the project a rule or the model proposes.
  misc: [
    { s: "Explain Rust lifetimes with examples", agent: "claude" },
    { s: "Compare two job queue libraries", agent: "claude", suggest: "B", source: "model" },
    { s: "Outline a talk on agent workflows", agent: "workbuddy" },
    { s: "Benchmark JSON parsers", agent: "codex", suggest: "C", source: "rule" },
    { s: "Clean up dotfiles", agent: "dsh" },
    { s: "Read the iOS 18 release notes", agent: "antigravity", suggest: "F", source: "model" },
  ],

  // What was worked on each day: project key and item numbers. "U" adds the next unassigned
  // session. Items marked with "!" arrived after that day's report was written.
  schedule: {
    "2026-10-26": ["B6", "C2", "D3"],
    "2026-10-27": ["B13", "C3", "F1"],
    "2026-10-28": ["B7", "D4"],
    "2026-10-29": ["B12", "F2", "U"],
    "2026-10-30": ["B16", "C5"],
    "2026-11-02": ["B9", "D5"],
    "2026-11-03": ["B8", "C4", "F3"],
    "2026-11-04": ["B15", "D6"],
    "2026-11-05": ["B10", "C6", "F4"],
    "2026-11-06": ["D7", "C7"],
    "2026-11-07": ["F5"],
    "2026-11-09": ["B14", "C8"],
    "2026-11-10": ["D8", "F6"],
    "2026-11-11": ["C9", "F7", "U"],
    "2026-11-12": ["B17", "D9", "C10", "F8"],
    "2026-11-13": ["D10", "F9"],
    "2026-11-15": ["F10"],
    "2026-11-16": ["B11", "U"],
    "2026-11-17": ["B1", "U"],
    "2026-11-18": ["B2", "B4!", "D1", "D2", "C1", "U"],
    "2026-11-19": ["B5", "B3", "B18"],
  },

  // What the person wrote, beside the generated reports (read-only in the demo). An entry is a
  // string or [text, [children]]. Per period: `day` notes on the period itself, then per project
  // key the notes at the `top` and `bottom` of its section and annotations (`ann`) keyed by the
  // index of the report point they hang on. `project` holds the notes on each project page.
  notes: {
    project: {
      B: [["Goal for November: no customer is ever charged twice.", ["Every path that moves money goes through the job queue", "Refunds get a second pair of eyes before they run"]]],
    },
    daily: {
      "2026-11-18": {
        day: ["Incident review in the morning; the afternoon was mine again."],
        projects: {
          B: {
            top: ["No double charges since the queue went live. Refunds are the last gap."],
            ann: { 0: ["Watched it in staging for an hour with duplicate webhooks: no double charges."] },
            bottom: [["Walked through the refund plan with Alex", ["Refund first, then email the affected customers", "Alex drafts the email; I review it tomorrow"]]],
          },
          D: { bottom: ["Sent the new quick start to two people who had never used the API. Both had a request working in under ten minutes."] },
          C: { ann: { 0: ["Tried it in fish myself: it works, but long descriptions get cut off."] } },
          F: { top: ["Site visit with the ops team: they take most notes with gloves on. The buttons need to be much bigger."] },
        },
      },
      "2026-11-19": {
        projects: {
          B: {
            top: ["Refunds go out on Friday once finance signs off."],
            ann: { 1: ["Checked five of the 37 by hand against the provider's dashboard: all real."] },
          },
        },
      },
    },
    weekly: {
      "2026-W47": {
        day: ["The double-charge week. Calmer by Thursday than it looked on Monday."],
        projects: { B: { top: ["Incident closed on Thursday; the postmortem went out to the team."] } },
      },
    },
  },

  // Report states on first load. Days not listed have a final report.
  reportState: {
    daily: { "2026-11-17": "draft", "2026-11-18": "draft", "2026-11-19": null },
    weekly: { "2026-W47": "draft" },
    monthly: { "2026-11": "draft" },
  },

  // Monthly reports are hand-written summaries rather than derived from items.
  monthly: {
    "2026-09": {
      B: ["Launched annual plans with a two-month discount", "Moved invoice PDF rendering to a background worker; checkout p95 down 30%", "Replaced hand-rolled currency tables with the provider's list"],
      D: ["Reorganised the guides into Get started, Build and Operate", "Added a runnable API playground"],
      C: ["First public release (v0.1) with login, list and upload", "Set up cross-compilation for macOS and Linux"],
      F: ["Prototyped the note editor and map view", "Chose SQLite over Core Data for the local store"],
    },
    "2026-10": {
      B: ["Upgraded the payment provider SDK and fixed JPY/KRW rounding", "Invoice PDFs now show tax per jurisdiction", "Stabilised the payment integration tests"],
      D: ["CI link checking and fixed search indexing for the guides", "Planned versioned docs for v2 and v3"],
      C: ["Config discovery and --json output for list commands", "Fixed Windows paths containing spaces"],
      F: ["Settled the offline sync model and built the sync queue"],
    },
    "2026-11": {
      B: ["Resolved the double-charge incident: idempotent job queue, refunds routed through it, 37 invoices refunded", "Proration for mid-cycle upgrades and a design for usage-based billing", "Invoice list 4.1 s → 180 ms; dunning schedule for failed payments"],
      D: ["Versioned docs with a v2 → v3 migration guide", "Code tabs, a lazy-loaded playground (Lighthouse 61 → 92) and a glossary"],
      C: ["Automated three-platform releases with a Homebrew formula", "Readable API errors, an update check and shell completion"],
      F: ["Photos, map clustering and a dark-mode pass; TestFlight 0.9 shipped", "Fixed the iOS 17 launch crash; idle battery use down about 40%"],
    },
  },

  // Filler turns for sessions, per project: a prompt, the outcome and the tools used.
  filler: {
    B: [
      { q: "Run the test suite and fix anything that fails.", a: "All tests pass after updating one fixture.", t: ["Bash go test ./...", "Edit internal/invoice/fixtures_test.go +4 −2"] },
      { q: "Show me where invoices are marked as paid.", a: "Payments are written in one transaction in invoice/service.go; the status flips at the end.", t: ["Read internal/invoice/service.go"] },
      { q: "Add a test for that case.", a: "Added a table-driven test with five cases.", t: ["Edit internal/invoice/retry_test.go +48"] },
      { q: "Tidy up the error messages before I open the PR.", a: "Errors now wrap the invoice id and keep the original cause.", t: ["Edit internal/invoice/errors.go +11 −6"] },
    ],
    D: [
      { q: "Preview the page locally.", a: "The dev server renders the page with no warnings.", t: ["Bash pnpm dev"] },
      { q: "Check the page on a narrow screen.", a: "Two code blocks overflowed; they now scroll inside their box.", t: ["Edit styles/code.css +6"] },
      { q: "Proofread the new section.", a: "Fixed four typos and one wrong flag name.", t: ["Edit content/guides/quick-start.mdx +4 −4"] },
    ],
    C: [
      { q: "Run cargo test and clippy.", a: "All tests pass; one clippy warning fixed.", t: ["Bash cargo test", "Bash cargo clippy --all-targets"] },
      { q: "Update the README for the new flag.", a: "README usage section updated with an example.", t: ["Edit README.md +9 −1"] },
      { q: "Try it against the staging API.", a: "Works against staging; output matches the docs.", t: ["Bash tidy list --json --env staging"] },
    ],
    F: [
      { q: "Build and run the tests on the simulator.", a: "Build succeeds; 112 tests pass.", t: ["Bash xcodebuild test -scheme FieldApp"] },
      { q: "Check it on the smallest supported screen.", a: "Layout holds on iPhone SE; one label truncated, fixed.", t: ["Edit FieldApp/Notes/NoteRow.swift +3 −1"] },
      { q: "Write a short note for the changelog.", a: "Added a changelog entry under Unreleased.", t: ["Edit CHANGELOG.md +3"] },
    ],
    U: [
      { q: "Can you give me a shorter version?", a: "Condensed it to five bullet points.", t: [] },
      { q: "Any trade-offs I should know about?", a: "Listed three trade-offs with a recommendation.", t: [] },
    ],
  },
};
