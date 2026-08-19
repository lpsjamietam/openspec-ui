## ADDED Requirements

### Requirement: Runtime Capability Reporting
The configuration API MUST report whether the running process is read-only and which listener, status provider, and deduplication behavior it uses.

#### Scenario: Frontend loads application capabilities
- **WHEN** the frontend requests configuration
- **THEN** the response contains `readOnly`, `bindAddress`, `statusProvider`, and `deduplicateChanges`

### Requirement: Read-Only Mutation Enforcement
All endpoints that write idea files or application configuration MUST reject requests while read-only mode is enabled.

#### Scenario: Client attempts a mutation in read-only mode
- **WHEN** a client sends POST, PUT, or DELETE to an idea endpoint or PUT to source configuration
- **THEN** the API returns `403 Forbidden` and no file changes

### Requirement: Worktree-Aware Change Responses
Change responses MUST expose status provenance, Git/track context, and duplicate-source membership without discarding the representative source.

#### Scenario: Change represents multiple worktrees
- **WHEN** identical changes are grouped
- **THEN** the response includes the representative source, duplicate count, and all contributing source identifiers
