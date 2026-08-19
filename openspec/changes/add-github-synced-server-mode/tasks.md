## 1. Configuration and Snapshot Contracts

- [x] 1.1 Add a provider-discriminated configuration model for existing filesystem mode and GitHub-only server mode, including repository, canonical Specs ref, Changes base ref, eligible PR targets, reconciliation interval, PR limit, and cache path.
- [x] 1.2 Load GitHub App identity, private key, installation identity, and webhook secret from environment variables or mounted secret files; add redaction-safe validation errors and prove secrets never appear in configuration responses.
- [x] 1.3 Introduce active-snapshot, source-provenance, synchronization-health, and archive-warning domain types with optional response fields that preserve filesystem API compatibility.
- [x] 1.4 Add configuration tests for unchanged legacy defaults, valid hosted mode, mixed-authority rejection, invalid ref/repository values, and missing secret-name-only diagnostics.

## 2. GitHub Read-Only Synchronization

- [x] 2.1 Add GitHub App JWT and short-lived installation-token acquisition with repository-scoped read permissions, expiry-aware caching, timeout handling, and credential-safe logs.
- [x] 2.2 Add GitHub repository and pull-request discovery that resolves the canonical ref plus open PR heads targeting configured branches and assigns deterministic source identities and provenance.
- [x] 2.3 Materialize only bounded `openspec/` content for each eligible immutable commit into staged cache generations while rejecting traversal paths, escaping symlinks, oversized files, and oversized aggregate snapshots.
- [x] 2.4 Force remote snapshots through filesystem-only OpenSpec parsing, reuse change deduplication, and associate canonical active changes with merged pull-request metadata without executing fetched repository content.
- [x] 2.5 Atomically validate and publish changed generations to memory and disk, retain the last-known-good generation on failure, prune superseded generations, and restore a valid cached generation during startup.
- [x] 2.6 Add GitHub client and snapshot tests using captured API/Git fixtures for ref filtering, fork PRs, token failures, malicious paths, partial fetch/parser failures, unchanged revisions, successful swaps, cache restoration, and merged-change association.

## 3. Refresh Orchestration and Webhooks

- [x] 3.1 Implement a bounded repository refresh queue and single synchronization worker that serializes refreshes, coalesces bursts, persists pending work, applies retry backoff, and prevents stale completion from overwriting newer state.
- [x] 3.2 Add startup and configurable periodic reconciliation, defaulting to 15 minutes, so a missed webhook or lost cache is repaired from current GitHub refs.
- [x] 3.3 Add the GitHub webhook route with raw-body limits, constant-time SHA-256 HMAC verification, repository/ref relevance filtering, and prompt acknowledgement without waiting for Git work.
- [x] 3.4 Persist a bounded delivery-ID ledger atomically and make duplicate, reordered, unsupported, invalid-signature, and irrelevant deliveries idempotent and observable.
- [x] 3.5 Add webhook and scheduler tests for supported push, pull-request lifecycle, installation-repository changes, replays, malformed payloads, signature failures, delivery deadlines, burst coalescing, restart recovery, and periodic fallback.

## 4. API, State, and Browser Updates

- [x] 4.1 Refactor Changes, Specs, source, and detail handlers to read one immutable active snapshot while preserving current filesystem-mode behavior and deduplication.
- [x] 4.2 Add a safe synchronization-health endpoint that reports active revision, contributing refs, last attempt/success, state, failure category, and last-known-good status without local paths or credential details.
- [x] 4.3 Extend Changes, Specs, and source responses with optional GitHub repository/ref/commit/PR provenance and the derived merged-but-unarchived warning.
- [x] 4.4 Emit exactly one SSE update after a changed generation is published and no update for failed or no-op reconciliation; add API/state tests for atomic visibility and concurrent readers.
- [x] 4.5 Add API regression tests proving filesystem consumers retain compatible response fields and hosted accepted Specs never come from pull-request sources.

## 5. Hosted Dashboard Experience

- [x] 5.1 Extend frontend types and data hooks for provider mode, GitHub provenance, synchronization health, and archive-warning metadata.
- [x] 5.2 Show GitHub source mode, canonical ref, last successful synchronization time, and initializing/healthy/degraded status in the header while retaining last-known-good content during failures.
- [x] 5.3 Show PR number, head/target refs, commit, and safe GitHub links on Change cards and details, and identify accepted Specs with only the configured canonical repository/ref.
- [x] 5.4 Show a merged-but-unarchived warning only for canonical active Changes at least 15 full days after the associated merge, including merge date and PR link, without changing task status or offering automatic archival.
- [x] 5.5 Add component and interaction tests for healthy, initializing, degraded, PR provenance, canonical Specs, exactly-before/at/after 15-day boundaries, archived changes, and absent merge association.

## 6. Container and Operations Contract

- [x] 6.1 Update the production container to include required Git functionality, run as a non-root user, expose the application port, and write only to a configurable mounted cache directory.
- [x] 6.2 Add a GitHub server-mode example configuration and deployment documentation covering GitHub App permissions, webhook events/secret, persistent cache, secret injection, health checks, restart behavior, and initial synchronization.
- [x] 6.3 Document and verify the required TLS/private authenticated ingress, webhook authentication bypass with signature validation and rate limits, conservative binding, and optional outbound restriction to GitHub.
- [x] 6.4 Add a container smoke test that starts in GitHub mode with fixture services, restores a cached snapshot, reaches healthy state after reconciliation, and remains read-only.

## 7. End-to-End Verification

- [x] 7.1 Run backend formatting, linting, unit/integration tests, and a release build, including secret-redaction and untrusted-content coverage.
- [x] 7.2 Run frontend formatting, linting, type checking, component tests, and production build.
- [x] 7.3 Exercise an end-to-end fixture flow from signed webhook through atomic sync, API provenance, SSE refresh, canonical Specs display, PR Change display, degraded recovery, and the 15-day archive warning.
- [x] 7.4 Re-run the complete existing filesystem-mode regression suite and manually verify that local unpublished worktrees remain available only in desktop mode.
- [x] 7.5 Run `openspec validate add-github-synced-server-mode --strict`, review the final diff against the proposal/specs/design, and record deployment prerequisites and any known limitations in the pull request.
