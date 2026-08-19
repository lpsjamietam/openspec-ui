## ADDED Requirements

### Requirement: Runtime Capability Reporting
The configuration API MUST report whether the running process is read-only and which listener, status provider, and deduplication behavior it uses.

#### Scenario: Frontend loads application capabilities
- **WHEN** the frontend requests configuration
- **THEN** the response contains `readOnly`, `bindAddress`, `statusProvider`, `deduplicateChanges`, and the optional `specsSourceId`

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

### Requirement: Canonical Specs API Boundary
When a canonical Specs source is configured, the Specs list and detail endpoints MUST expose specifications only from that source.

#### Scenario: Client requests all specifications
- **WHEN** `specsSourceId` selects one valid source and the client requests `/api/specs`
- **THEN** every returned specification belongs to the selected source

#### Scenario: Client requests a worktree specification directly
- **WHEN** `specsSourceId` selects one source and a detail request names a different source
- **THEN** the API returns `404 Not Found`
