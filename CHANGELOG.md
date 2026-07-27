# Changelog

All notable changes to this project. Format: [Keep a Changelog](https://keepachangelog.com); scheme: [SemVer](https://semver.org).

The version number lives in `VERSION` and is stamped on `main` at merge — pull requests add entries under `[Unreleased]` and leave the number alone.

## [Unreleased]

## [0.2.0] - 2026-07-27

### Fixed
- **Changes created by OpenSpec 1.0+ were invisible on the board.** A directory was only treated as a change if it contained `proposal.md`, but current OpenSpec writes `.openspec.yaml` when the change is created and the proposal only once an agent drafts it. Every freshly created change was therefore missing from the dashboard until someone wrote a proposal — the exact moment you most want to see it. Either marker now identifies a change.

### Added
- **Artifact chain per change.** Each change reports `proposal`, `design`, `specs` and `tasks` as `complete`, `ready` (dependencies met, not written yet) or `blocked`, with the artifacts it is waiting for. Vocabulary and dependency graph match `openspec status --change <name>`, verified against OpenSpec 1.6, but are computed from the files on disk — so no OpenSpec install is needed in the repos being watched. Exposed on `/api/changes` and `/api/changes/{id}`, and rendered on each card with the next actionable artifact marked.
- CI on every push and pull request: frontend build, `cargo test`, `cargo clippy -D warnings`. Nothing verified commits before this.
- `release.yml` builds and publishes release archives from a `v*` tag for four targets (adds macOS Intel), replacing manual assembly from workflow artifacts.
- `CHANGELOG.md`, following the ToruAI versioning standard.

### Changed
- Cards show the whole workflow chain instead of badges for artifacts that happen to exist, so a glance answers "what is next here" rather than only "what has been written".
- Consolidated four dispatch-only workflows (`ci-all`, `ci-linux`, `ci-macos`, `ci-windows`) into `ci.yml` + `release.yml`. They built the same binaries but only uploaded them as build artifacts, so publishing a release meant downloading and re-uploading by hand. `release.yml` keeps a `workflow_dispatch` entry point for builds without a tag.
- README leads with what the dashboard answers and states OpenSpec version compatibility; download table no longer hard-codes `v0.1.0`.

### Removed
- `readyForReview` from the `Change` type: declared in the frontend contract, never sent by the backend, never read by any component.

## [0.1.0] - 2026-01-07

### Added
- Multi-repo kanban dashboard for OpenSpec changes, with ideas capture, specs browser, detail view and SSE live refresh.
- Single-binary distribution (embedded frontend) for macOS, Linux and Windows; Docker image.
