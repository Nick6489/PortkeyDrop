# Maintaining PortkeyDrop

The canonical repository is [Nick6489/PortkeyDrop](https://github.com/Nick6489/PortkeyDrop).
The application is a Rust workspace. Python remains only for release-note tooling
and its tests; the old Python application is preserved in Git history.

## Everyday development

1. Fetch the latest branches and start a `feature/*` or `fix/*` branch from `origin/dev`.
2. Make the change and add user-facing notes under `## Unreleased` in `CHANGELOG.md`.
3. Run the checks below. Use Conventional Commit messages and PR titles.
4. Open a PR targeting `dev`, review its changes and CI results, then merge deliberately.
   Do not enable auto-merge. When using GitHub CLI, supply the description with `--body-file`.

```powershell
git fetch origin
git switch -c fix/descriptive-name origin/dev
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m pytest tests/test_changelog_tools.py -q
```

Rust stable and a C++ build toolchain are required. The first full build compiles
wxWidgets and can be slow. Core-only work can be checked with
`cargo test -p portkeydrop-core`; CI also checks the full application on Windows
and the core on Ubuntu. Live SSH tests need the optional SSH harness configured.

For changes without a user-facing effect, put `Changelog: none` or
`[skip changelog]` on its own line in every commit. A `skip-changelog` PR label
skips the PR gate but does not exempt the subsequent push to `dev`.

## Nightly builds

The [Build workflow](../.github/workflows/build.yml) runs daily at 00:27 UTC.
GitHub loads the scheduled workflow from the default branch, `main`, then the
workflow checks out `dev` for its source. Workflow edits on `dev` do not change
the scheduled workflow until promoted to `main`.

The schedule builds only when the changelog has entries not already announced
by the previous nightly or latest stable release, or when a commit explicitly
requests a build with `nightly: build` or `[nightly build]` on its own line.
A successful run with skipped platform jobs is normal when nothing needs announcing.

Manual runs bypass that gate. Select `dev` for a manual nightly. A manual run on
another branch builds that branch, but nightly release creation still targets
`dev`, so do not publish manual nightlies from feature branches. Use `dry_run`
for packaging validation without creating a release. It still runs the build
and artifact jobs. Repeating publication on the same UTC date replaces that
date's nightly release and tag.

Nightlies have `nightly-YYYYMMDD` tags and are GitHub prereleases. The build
embeds that date so the updater can distinguish nightlies sharing a version.
Release notes come from `CHANGELOG.md`, and assets include a checksum file.

Windows produces an installer and portable ZIP; macOS produces a DMG. Linux
builds remain enabled, but uploads require the repository variable
`PUBLISH_LINUX=true`. Leave it unset until compatibility is verified on supported
distributions. Release publication can proceed after only one platform succeeds:
inspect individual jobs and attached downloads, not just the release's existence.

## Stable releases and workflow promotion

`main` is the stable/default branch and `dev` is the integration branch. History
uses deliberate promotions from `dev` into `main` before version tags. Ordinary
PRs must target `dev`; the repository instructions do not grant a general
exception for release PRs to `main`. Obtain an explicit maintainer decision for
a promotion, including whether to promote all of `dev` or only a workflow fix.

Before publishing a stable release:

1. Review the complete changes since the previous stable tag and check CI.
2. Prepare the intended version in workspace `Cargo.toml` and refresh `Cargo.lock`;
   move the release notes into the matching version section in `CHANGELOG.md`.
3. Validate packaging with a dry run and manually check startup, accessibility,
   connections, transfers, and upgrading an existing installation on supported platforms.
4. Promote the approved changes into `main` using the agreed release procedure.
5. Only then push the matching `vX.Y.Z` tag to publish. The workflow accepts version
   tags from any branch and does not enforce that they belong to `main`, so check
   the target commit and version before pushing.
6. Inspect all platform results, downloads, checksums, and release notes. Carry
   release-preparation changes back into `dev` if they were made separately.

Changing source or merging a PR does not publish a stable release. Pushing a
matching version tag does. Avoid reusing an existing stable tag: rerunning its
workflow can overwrite that release's assets.

## Ownership transition notes

As checked on September 19, 2026, the latest stable release is `v0.6.0` from
July 24 and contains the Python application. Rust development still uses the
same version number; a new stable Rust release needs an intentional version
decision. Do not mistake the workspace version for proof a Rust stable shipped.

The old website notification was removed on `dev` in PR #155. Until that workflow
change reaches `main`, scheduled runs retain the old notification step, which
sends only if `ORINKS_BUILD_NOTIFICATION_TOKEN` is configured. Audit obsolete
repository secrets during the handover without copying secret values into logs.

Updater URLs, public links, and installer publisher metadata should name the new
owner. Preserve the installer AppId, Windows assembly identity, and macOS bundle
identifier during this transition: they identify the existing installed app and
are not links to the former owner's services. Preserve historical attribution too.

At the handover, `main` and `dev` had no branch protection and no repository
rulesets. Review protection separately after CI check names and the desired
single-maintainer workflow are settled; these written rules are not enforced
by GitHub settings.
