## Context
OpenSpec UI currently derives one fixed artifact graph directly from files, treats configured paths as unrelated repositories, exposes mutating routes despite describing itself as read-only, and listens on every interface. The intended personal workflow uses many short-lived worktrees and OpenSpec CLI 1.9 or newer.

## Goals / Non-Goals
- Goals: safe local defaults, authoritative artifact status when available, visible Git/track context, and useful duplicate suppression.
- Non-goals: GitHub/PR synchronization, automatic target-branch inference, repository discovery, or writing OpenSpec artifacts.

## Decisions
- Decision: configuration defaults to `readOnly: true`, `bindAddress: "127.0.0.1"`, `deduplicateChanges: true`, and `statusProvider: "auto"`.
  - Rationale: first-run behavior should be safe and useful on a developer laptop while preserving explicit opt-outs.
- Decision: `auto` invokes `openspec status --change <name> --json` for active changes and falls back to filesystem inference when the executable or output is unavailable.
  - Rationale: the installed CLI owns schema evolution; the fallback preserves the upstream zero-install use case.
- Decision: every change reports `statusSource`, including whether artifact state came from `cli` or `filesystem`.
  - Rationale: fallback must be observable rather than silently authoritative.
- Decision: duplicate identity is the stable hash of the change name and sorted artifact file paths/content. The first configured source is the representative and all matching sources remain listed.
  - Rationale: copied OpenSpec trees collapse, while independently changed artifacts remain distinct.
- Decision: Git context is discovered from the source path, while `track` and `targetBranch` remain explicit optional source configuration.
  - Rationale: the current branch is factual; delivery ownership and intended merge target cannot be safely inferred.
- Decision: an optional `specsSourceId` selects the single source used by both Specs list and detail endpoints; omitting it preserves the multi-source upstream behavior.
  - Rationale: accepted specifications have one canonical branch, while proposed changes may legitimately span many local worktrees. Enforcing the selection in the API prevents a direct detail request from bypassing the UI filter.

## Risks / Trade-offs
- Spawning the CLI adds latency per distinct active change. Duplicate suppression happens before CLI enrichment so copied worktrees incur one command.
- Content-based deduplication does not detect equivalent artifacts with formatting-only differences; avoiding false merges is preferred.
- Read-only mode cannot prevent an administrator from changing mounted files outside the process; filesystem read-only mounts remain the strongest deployment boundary.

## Migration Plan
Existing JSON configuration files require no edits. Users who need idea or settings mutation must explicitly set `readOnly` to `false`. Users who intentionally expose the server must explicitly set `bindAddress`.
Users who want a canonical Specs browser set `specsSourceId` to one configured source name. Each machine still supplies its own filesystem paths; only committed repository content travels through Git.
