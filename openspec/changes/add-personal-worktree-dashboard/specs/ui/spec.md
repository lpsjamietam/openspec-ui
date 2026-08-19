## ADDED Requirements

### Requirement: Visible Read-Only Operation
The UI MUST show when the application is read-only and MUST not present controls that imply mutations are available.

#### Scenario: Dashboard runs read-only
- **WHEN** runtime configuration reports `readOnly: true`
- **THEN** the header identifies read-only mode, idea creation is hidden, and settings cannot be saved

### Requirement: Worktree Context on Changes
The dashboard MUST distinguish the factual source branch from configured delivery ownership and target branch.

#### Scenario: Change comes from a tracked feature worktree
- **WHEN** Git branch, track, and target branch metadata are available
- **THEN** the card and detail view display the branch and a clear track-to-target relationship

#### Scenario: Source is detached
- **WHEN** Git discovery reports a detached worktree
- **THEN** the UI labels it as detached instead of presenting the commit as a branch

### Requirement: Transparent Duplicate and Fallback States
The dashboard MUST disclose grouped worktrees and filesystem status fallback.

#### Scenario: Identical worktree copies are grouped
- **WHEN** a change represents multiple configured sources
- **THEN** the card shows the number of grouped copies and the detail view lists their source identifiers

#### Scenario: CLI enrichment fails
- **WHEN** a change reports filesystem status provenance while automatic CLI status is configured
- **THEN** the UI marks the status as a fallback rather than implying CLI authority
