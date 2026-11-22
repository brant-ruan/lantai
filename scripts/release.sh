#!/usr/bin/env bash
# Prepares a release: sets the workspace version, refreshes Cargo.lock and turns the
# "Unreleased" section of CHANGELOG.md into a dated release section.
# It does not commit, tag or push; it prints those commands for you to review and run.
#
# Usage: scripts/release.sh <version>      e.g. scripts/release.sh 0.2.0
set -euo pipefail

REPO_URL="https://github.com/brant-ruan/lantai"

die() { echo "error: $*" >&2; exit 1; }

[ $# -eq 1 ] || die "usage: scripts/release.sh <version>"
NEW="${1#v}"
[[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "'$NEW' is not a SemVer version (MAJOR.MINOR.PATCH)"

cd "$(git rev-parse --show-toplevel)"
[ -z "$(git status --porcelain)" ] || die "the working tree has uncommitted changes"
git rev-parse -q --verify "refs/tags/v$NEW" >/dev/null && die "tag v$NEW already exists"

OLD="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"
[ -n "$OLD" ] || die "could not read [workspace.package] version from Cargo.toml"

# The new version must sort after the current one (pre-release suffixes compare loosely).
LATEST="$(printf '%s\n%s\n' "$OLD" "$NEW" | sort -V | tail -n1)"
[ "$LATEST" = "$NEW" ] || die "$NEW is lower than the current version $OLD"

# The Unreleased section must say what is in the release.
NOTES="$(awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{f=0} f' CHANGELOG.md | grep -v '^[[:space:]]*$' || true)"
[ -n "$NOTES" ] || die "CHANGELOG.md has nothing under ## [Unreleased]"

PREV_TAG="$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)"
TODAY="$(date +%Y-%m-%d)"

# 1. Version: the workspace version is the only place it is written.
if [ "$OLD" != "$NEW" ]; then
  perl -0pi -e "s/(\[workspace\.package\]\nversion = )\"\Q$OLD\E\"/\${1}\"$NEW\"/" Cargo.toml
fi
cargo metadata --format-version 1 --offline >/dev/null # refreshes Cargo.lock

# 2. Changelog: open an empty Unreleased section above the new release.
perl -0pi -e "s/^## \[Unreleased\]\n/## [Unreleased]\n\n## [$NEW] - $TODAY\n/m" CHANGELOG.md
if [ -n "$PREV_TAG" ]; then
  NEW_LINK="[$NEW]: $REPO_URL/compare/$PREV_TAG...v$NEW"
else
  NEW_LINK="[$NEW]: $REPO_URL/releases/tag/v$NEW"
fi
perl -0pi -e "s|^\[Unreleased\]: .*\$|[Unreleased]: $REPO_URL/compare/v$NEW...HEAD\n$NEW_LINK|m" CHANGELOG.md

echo "Prepared v$NEW (was $OLD). Review the diff, then run:"
echo
echo "  git commit -am \"chore: release v$NEW\""
echo "  git tag -a v$NEW -m \"Lantai v$NEW\""
echo "  git push origin main v$NEW"
