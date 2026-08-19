## ADDED Requirements

### Requirement: Safe Local Defaults
The application MUST default to read-only behavior and a loopback listener when configuration fields are omitted.

#### Scenario: Existing configuration omits safety fields
- **WHEN** the application loads an existing configuration containing only sources and a port
- **THEN** mutation endpoints are disabled and the listener binds to `127.0.0.1`

#### Scenario: Operator explicitly opts into mutation and network exposure
- **WHEN** configuration sets `readOnly` to `false` and supplies a non-loopback `bindAddress`
- **THEN** the application uses those explicit values

### Requirement: Worktree Source Metadata
Each configured source MAY declare a delivery track and target branch, and the application MUST discover factual Git worktree context when available.

#### Scenario: Source belongs to a Git worktree
- **WHEN** a source path resolves inside a Git worktree
- **THEN** the source reports its worktree root, branch or detached state, and current commit

#### Scenario: Delivery ownership is configured
- **WHEN** a source declares `track` and `targetBranch`
- **THEN** those values are returned without attempting to infer or replace them

### Requirement: Status Provider Selection
The application MUST support `auto` and `filesystem` status providers, with `auto` preferring the installed OpenSpec CLI and falling back safely.

#### Scenario: Compatible OpenSpec CLI is available
- **WHEN** `statusProvider` is `auto` and the CLI returns valid status JSON
- **THEN** artifact states use the CLI response and report CLI provenance

#### Scenario: CLI status is unavailable
- **WHEN** the command is absent, fails, or returns invalid JSON
- **THEN** the application retains filesystem-derived states and reports filesystem provenance

### Requirement: Duplicate Change Suppression
The application MUST be able to collapse changes whose names and artifact content are identical across configured sources.

#### Scenario: Identical change exists in copied worktrees
- **WHEN** deduplication is enabled and multiple sources contain identical artifacts for the same change
- **THEN** one change is returned with every contributing source identified

#### Scenario: Same-named changes have different artifacts
- **WHEN** artifact content differs between sources
- **THEN** each change remains independently visible

### Requirement: Canonical Specs Source
The application MUST support designating one configured source as the canonical source for accepted specifications while continuing to aggregate proposed changes across every valid source.

#### Scenario: Operator selects the demo base
- **WHEN** configuration sets `specsSourceId` to the source representing `demo/main`
- **THEN** Specs list and detail requests use only that source while Changes continue to include configured feature worktrees

#### Scenario: Existing configuration omits the selection
- **WHEN** `specsSourceId` is absent
- **THEN** the application preserves the existing multi-source Specs behavior
