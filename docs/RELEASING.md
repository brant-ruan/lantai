# Versioning and releases

## Semantic versioning

Lantai uses [Semantic Versioning 2.0](https://semver.org/) (`MAJOR.MINOR.PATCH`), not date-based
versions:

1. **Tooling expects it:** Cargo crate versions, Tauri's `version` and, later, the updater plugin
   all compare SemVer.
2. **Lantai has contracts:** the IR, the database, `config.toml` and the `aj` CLI are interfaces.
   A version number should tell users whether an upgrade can break their data or scripts; a date
   only says when it shipped.
3. **Releases follow features, not a calendar.** Dates are in the changelog and on Git tags.

## Rules before 1.0

Nothing is promised stable before `1.0.0`:

| Bump | When |
| :--- | :--- |
| `0.MINOR.0` | new features, or any breaking change: a new `SCHEMA_VERSION`, a new IR version, a `config.toml` format change, a renamed or removed CLI argument |
| `0.x.PATCH` | fixes only, no data format or interface changes |

Breaking changes are listed under **Breaking** in the changelog with what users must do. They are
decided case by case by the project owner, never by default (no "delete the database and
rescan"); see the second principle in `AGENTS.md`.

`1.0.0` comes when the IR, database and configuration formats have migrations and a compatibility
promise.

## Where the version lives

The single source is `[workspace.package] version` in the root `Cargo.toml`.

- Every crate uses `version.workspace = true`.
- `app/src-tauri/tauri.conf.json` has no `version`; Tauri falls back to Cargo's (the bundled
  `Info.plist` is correct).
- `app/package.json` is private and has no `version`.
- Shown by `aj --version` and in Settings → About.

Internal formats have their own versions, independent of the app version; each changelog section
states the values at the time:

| Name | Where | Form |
| :--- | :--- | :--- |
| IR | `IR_VERSION` in `crates/aj-ir/src/lib.rs` | SemVer |
| Database schema | `SCHEMA_VERSION` in `crates/aj-store/src/schema.rs` | integer |
| Digest and cluster prompts | `DIGEST_PROMPT_VERSION`, `BRIEF_PROMPT_VERSION` | string, cache invalidation only |

## Changelog

`CHANGELOG.md` is in English and follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

- When a user-visible change lands on `main`, add an entry under `## [Unreleased]` (Added /
  Changed / Fixed / Removed / Breaking). Internal refactoring, tests and docs need none.
- When `IR_VERSION` or `SCHEMA_VERSION` changes, update the "IR x · Schema y" line at the top of
  Unreleased and explain under Breaking.

## Releasing

On a clean `main`:

```bash
scripts/release.sh 0.2.0
```

The script checks that the version is well formed and greater than the current one, that the tree
is clean and Unreleased is not empty; then it updates `Cargo.toml` and `Cargo.lock`, turns
Unreleased into a dated section and updates the links at the bottom. It **does not commit, tag or
push**; it prints the commands to run after reviewing the diff:

```bash
git commit -am "chore: release v0.2.0"
git tag -a v0.2.0 -m "Lantai v0.2.0"
git push origin main v0.2.0
```

Tags are always `v` + version. Pushing the tag runs `.github/workflows/release.yml`, which:

1. checks that the tag matches the workspace version;
2. builds a universal (Apple silicon and Intel) `Lantai.app` and `.dmg`, with `aj` built, merged
   and embedded by `scripts/bundle-cli.sh`;
3. ad-hoc signs and verifies it (no Apple Developer ID yet, so no notarization);
4. creates a draft release whose notes are the changelog section plus instructions for opening an
   unsigned app, with the `.dmg` attached.

Check the draft (install the `.dmg` on a Mac), then publish it by hand. Only published releases
are seen by the app's update check; drafts and prereleases are ignored.

Running the workflow by hand on a branch only builds the `.dmg` and uploads it as a build artifact,
without a release; use it to test packaging before a release.
