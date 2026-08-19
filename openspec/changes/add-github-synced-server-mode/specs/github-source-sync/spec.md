## Purpose

Provides a continuously refreshed, read-only GitHub source for hosted OpenSpec dashboards while preserving a last-known-good view during remote failures.

## ADDED Requirements

### Requirement: GitHub-only hosted source
The application MUST support a hosted mode in which GitHub is the only authoritative source and developer worktrees are not scanned.

#### Scenario: Hosted mode starts successfully
- **WHEN** hosted mode has valid repository access and a successful initial synchronization
- **THEN** the active snapshot contains accepted specifications from the configured Specs ref and proposed changes from eligible GitHub refs only

#### Scenario: Filesystem mode remains selected
- **WHEN** hosted mode is not configured
- **THEN** existing filesystem source discovery and behavior remain unchanged

### Requirement: Canonical Specs and pull-request Changes
The hosted snapshot MUST read accepted Specs from the configured canonical ref and MUST include Changes from that ref plus open pull-request heads targeting configured branches.

#### Scenario: Pull request targets the configured demo base
- **WHEN** an open pull request targets `demo/main`
- **THEN** its head ref is eligible to contribute proposed Changes while accepted Specs continue to come only from the canonical Specs ref

#### Scenario: Pull request targets another branch
- **WHEN** an open pull request does not target any configured target branch
- **THEN** its head ref is excluded from the hosted snapshot

### Requirement: Least-privilege repository access
The application MUST authenticate hosted synchronization using read-only GitHub App installation access and MUST NOT require repository write permission.

#### Scenario: Installation token is available
- **WHEN** a synchronization needs GitHub API or Git access
- **THEN** the application uses a short-lived installation token scoped to the configured repository and read permissions

#### Scenario: Authentication cannot be refreshed
- **WHEN** a valid installation token cannot be obtained
- **THEN** synchronization fails without changing the active snapshot or exposing credential material

### Requirement: Event-driven and periodic reconciliation
The application MUST schedule repository reconciliation for relevant GitHub events and MUST also reconcile periodically to recover from missed deliveries.

#### Scenario: Relevant push arrives
- **WHEN** a verified push updates the canonical Specs ref or an eligible pull-request ref
- **THEN** the application schedules synchronization for the repository

#### Scenario: Pull-request lifecycle changes
- **WHEN** an eligible pull request is opened, reopened, synchronized, retargeted, or closed
- **THEN** the application refreshes the eligible ref set and schedules synchronization

#### Scenario: Webhook delivery is missed
- **WHEN** no webhook triggers a refresh before the configured reconciliation interval elapses
- **THEN** periodic reconciliation compares GitHub state and publishes an updated snapshot when required

### Requirement: Atomic last-known-good snapshots
The application MUST publish a synchronized snapshot atomically and MUST retain the previous successful snapshot when fetch, validation, or parsing fails.

#### Scenario: Synchronization succeeds
- **WHEN** all eligible refs are fetched and parsed successfully
- **THEN** the application replaces the active snapshot as one operation and emits one browser update notification

#### Scenario: Synchronization fails
- **WHEN** any required canonical-ref fetch or snapshot validation fails
- **THEN** the last successful snapshot remains available and synchronization health reports the failure

### Requirement: Untrusted pull-request isolation
The application MUST treat pull-request content as untrusted data and MUST NOT execute repository code while producing the hosted snapshot.

#### Scenario: Pull request changes executable files
- **WHEN** an eligible pull request contains code, scripts, hooks, or build configuration
- **THEN** synchronization reads only the OpenSpec content needed for the dashboard and does not execute the changed repository content
