## ADDED Requirements

### Requirement: GitHub server-mode configuration
The application MUST accept GitHub server-mode configuration that identifies the repository, canonical Specs ref, Changes base ref, eligible pull-request target branches, synchronization interval, and persistent snapshot cache location.

#### Scenario: Complete GitHub configuration is supplied
- **WHEN** all required GitHub server-mode fields are valid
- **THEN** the application starts in GitHub-only hosted mode and does not require filesystem sources

#### Scenario: Filesystem and GitHub modes conflict
- **WHEN** configuration attempts to select both filesystem sources and GitHub-only hosted mode as authoritative
- **THEN** startup fails with a clear configuration error instead of combining the modes implicitly

### Requirement: Hosted secrets remain external
GitHub App credentials and the webhook signing secret MUST be loaded from deployment secrets and MUST NOT be serialized in configuration API responses or logs.

#### Scenario: Configuration API is requested
- **WHEN** hosted mode is active and a client requests runtime configuration
- **THEN** the response reports non-secret GitHub capabilities and repository selection without returning private keys, installation tokens, or webhook secrets

#### Scenario: Required secret is missing
- **WHEN** hosted mode starts without a required credential or webhook secret
- **THEN** startup fails before accepting webhook or synchronization work and identifies only the missing secret name

### Requirement: Filesystem compatibility
Existing configuration files that omit GitHub server mode MUST retain their current defaults and filesystem behavior.

#### Scenario: Existing personal-worktree configuration loads
- **WHEN** an existing configuration contains filesystem sources and no GitHub section
- **THEN** the application loads those sources with no migration requirement
