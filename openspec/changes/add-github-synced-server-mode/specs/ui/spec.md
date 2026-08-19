## ADDED Requirements

### Requirement: GitHub provenance presentation
The hosted dashboard MUST identify the repository ref, commit, and pull request that contributed each displayed Change while keeping accepted Specs visibly tied to the canonical Specs ref.

#### Scenario: Change comes from an open pull request
- **WHEN** a Change is contributed by an eligible pull-request head
- **THEN** its card and detail view show the pull-request number, head ref, target ref, commit, and a link to GitHub

#### Scenario: Accepted Spec is displayed
- **WHEN** a user browses Specs in hosted mode
- **THEN** the interface identifies the canonical repository and ref and does not present pull-request delta specs as accepted Specs

### Requirement: Synchronization health presentation
The hosted dashboard MUST show the last successful synchronization time and MUST make degraded or stale snapshot state visible without replacing the retained content.

#### Scenario: Synchronization is current
- **WHEN** the active snapshot is healthy
- **THEN** the header shows the last successful synchronization time and GitHub source mode

#### Scenario: Last refresh failed
- **WHEN** the server is serving a last-known-good snapshot
- **THEN** the interface shows a degraded warning with the last successful time and safe failure summary

### Requirement: Merged change archive warning
The hosted dashboard MUST flag a Change that remains active on the canonical base 15 days after its associated pull request was merged and MUST NOT archive or modify it automatically.

#### Scenario: Archive grace period expires
- **WHEN** an active Change on the canonical base is associated with a pull request merged at least 15 days earlier
- **THEN** its card and detail view show a merged-but-unarchived warning and the merge date

#### Scenario: Change is archived during the grace period
- **WHEN** the Change moves to the archive before 15 days elapse
- **THEN** no merged-but-unarchived warning is shown
