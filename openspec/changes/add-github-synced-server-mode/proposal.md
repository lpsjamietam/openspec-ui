## Why

The shared dashboard currently depends on workstation-specific filesystem paths, so it cannot provide a trustworthy, continuously updated view when hosted for a team. A GitHub-synced server mode gives the hosted instance one remote source of truth while retaining the existing local worktree mode for unpublished desktop work.

## What Changes

- Add a GitHub-only server mode that reads accepted Specs from a configured base ref such as `demo/main` and proposed Changes from that base plus open pull-request heads targeting configured branches.
- Authenticate repository reads through a least-privilege, read-only GitHub App and keep credentials outside the JSON configuration.
- Receive signed `push`, `pull_request`, and installation-repository webhooks, deduplicate deliveries, enqueue refreshes, and return promptly without performing Git work in the webhook request.
- Reconcile GitHub state periodically as a recovery path, preserve the last successful snapshot on refresh failure, and expose synchronization health and provenance.
- Reuse the existing parser, content deduplication, API responses, and SSE browser refresh after atomically publishing a new remote snapshot.
- Flag a merged change that remains active and unarchived for 15 days without automatically writing to the repository or archiving it.
- Document a container-ready deployment behind TLS and an authenticated private ingress. The application remains read-only and does not add viewer authentication in this change.
- Preserve existing filesystem configuration and behavior when GitHub server mode is not enabled.

## Capabilities

### New Capabilities

- `github-source-sync`: Read-only GitHub repository and pull-request synchronization, webhook processing, periodic reconciliation, snapshot publication, and failure recovery.

### Modified Capabilities

- `config`: Add mutually exclusive GitHub server-mode configuration while preserving filesystem defaults and keeping secrets outside configuration responses.
- `api`: Add webhook ingestion and synchronization-health behavior while continuing to serve Changes, Specs, and SSE from the active snapshot.
- `ui`: Show GitHub ref/PR provenance, last-sync health, and merged-but-unarchived warnings in the hosted dashboard.

## Impact

- Backend configuration, application state, source loading, Git integration, webhook routes, synchronization jobs, and SSE update signaling.
- Frontend API contracts, header/status surfaces, Change cards/details, and error/degraded-state presentation.
- New GitHub API/authentication and webhook-signature dependencies, plus persistent cache storage for a last-known-good repository snapshot and delivery IDs.
- Deployment configuration for GitHub App credentials, webhook secret, cache volume, TLS, and external viewer access control.
- Automated tests for configuration compatibility, GitHub event validation, ref filtering, synchronization recovery, provenance, and the 15-day archive warning.
