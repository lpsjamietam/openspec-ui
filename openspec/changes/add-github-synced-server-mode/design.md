## Context

See `proposal.md` for motivation. Today the backend resolves configured filesystem paths into `Source` values, parses Changes and Specs on each request, optionally asks the installed OpenSpec CLI for status, and broadcasts filesystem updates over SSE. That works for a personal workstation but does not give a hosted process a canonical remote view or durable recovery state.

The hosted variant must remain read-only, accept untrusted pull-request content without executing repository code, continue serving a useful last-known-good view during GitHub failures, and coexist with the current filesystem configuration. The project currently has no external database, so synchronization metadata must remain operable with a mounted cache volume. Hosting-provider selection is intentionally deferred; this design produces a portable container contract.

## Goals / Non-Goals

**Goals:**

- Add one source-provider boundary so request handlers consume an immutable active snapshot regardless of whether it came from local paths or GitHub.
- Build a deterministic GitHub snapshot from one accepted-spec ref and the relevant open pull-request heads.
- Make webhook refresh fast, authenticated, idempotent, recoverable, and observable.
- Preserve the existing parser, deduplication rules, API shapes where compatible, and SSE refresh behavior.
- Keep credentials and repository writes outside the dashboard.

**Non-Goals:**

- Selecting or provisioning a specific cloud vendor, DNS name, TLS certificate, or identity provider.
- Adding viewer accounts, repository write operations, automatic merges, or automatic OpenSpec archival.
- Combining hosted GitHub data with unpublished workstation files. The two-source comparison remains a desktop-only concern.
- Generalizing the service into a multi-tenant Git hosting platform or supporting providers other than GitHub.

## Decisions

### 1. Make filesystem and GitHub mutually exclusive source providers

Introduce a provider-level configuration discriminator. The existing filesystem provider retains `sources`, `specsSourceId`, watcher behavior, and optional CLI enrichment. The GitHub provider instead supplies an application-owned `ActiveSnapshot` containing resolved source directories, provenance, sync health, and a monotonically changing revision.

Handlers such as Changes, Specs, details, and sources read the active snapshot rather than discovering remote state during an HTTP request. Existing filesystem configuration remains valid without migration. GitHub-only fields are rejected when mixed with filesystem authority so an operator cannot accidentally present two competing truths.

Alternative considered: extend every existing `SourceConfig` with remote fields. That makes accepted Specs, base Changes, and pull-request heads look equivalent even though they have different lifecycle and filtering rules, and it leaves request handlers responsible for synchronization.

### 2. Materialize only OpenSpec content into immutable snapshot generations

The synchronizer uses GitHub App installation credentials to fetch the configured repository refs, but publishes only the repository's OpenSpec tree into an application-owned cache generation. It creates:

- one canonical source from the configured Specs/base ref;
- one source per open pull request whose base branch matches the configured target set; and
- snapshot metadata containing repository, ref, commit SHA, pull-request identity, timestamps, and synchronization state.

Pull-request heads are keyed by pull-request number and immutable head SHA, including fork pull requests that the installation can read. Source IDs are deterministic and do not depend on temporary paths. The backend parser reads the staged OpenSpec directories, but the service never runs scripts, hooks, builds, or binaries from fetched repository content. Remote mode forces filesystem status calculation rather than invoking a repository-provided OpenSpec command.

A generation is parsed and validated before publication. Publication atomically swaps the in-memory active snapshot and an on-disk current-generation pointer, then emits one SSE update only when the content revision changed. Old generations are pruned after the swap. A failed refresh records degraded health but leaves both active pointers untouched.

Alternative considered: serve files directly through the GitHub Contents API on every UI request. That increases latency and rate-limit exposure, prevents atomic cross-ref views, and gives no last-known-good behavior. A full working checkout was also rejected because it stores and exposes unrelated repository content and increases the risk of executing untrusted files.

### 3. Use a read-only GitHub App with short-lived installation tokens

The server authenticates as a GitHub App and mints short-lived installation access tokens when synchronization requires them. The app requests only repository metadata, Contents read, and Pull requests read permissions. Tokens are cached only until shortly before expiry, are never placed in Git remote URLs or serialized API responses, and are redacted from errors and structured logs.

Configuration contains repository identity and non-secret tuning only. App identity, private key, installation identity, and webhook secret come from environment variables or mounted secret files. Startup fails with a secret-name-only diagnostic when required values are absent.

Alternative considered: a personal access token. It is simpler initially but ties the deployment to a user, is typically longer lived, and makes least-privilege installation and revocation harder.

### 4. Treat webhooks as authenticated invalidation hints, not as source data

Add a dedicated webhook route that reads the raw body under a strict size limit, validates the SHA-256 HMAC before JSON processing, and checks a persisted bounded ledger of GitHub delivery IDs. Accepted `push`, `pull_request`, and `installation_repositories` events are reduced to repository-scoped refresh reasons. Relevant deliveries are durably queued/coalesced before returning a success response; duplicate deliveries return success without adding work. Invalid signatures return an authentication error, and unsupported or irrelevant events return success without refresh.

A single repository synchronization worker serializes refreshes and coalesces bursts so it cannot publish older data over newer data. Webhook handling performs no Git transfer and returns comfortably within GitHub's delivery deadline. A configurable periodic reconciliation, defaulting to 15 minutes, refreshes all relevant refs and repairs missed webhook events. Startup first serves a valid cached generation when present and immediately schedules reconciliation.

The delivery ledger and pending refresh marker are atomically persisted in the cache volume as bounded JSON metadata; an external database or message broker is not required for the first deployment.

Alternative considered: GitHub Actions calling a refresh endpoint after repository workflows. That duplicates credentials and workflow setup, misses changes when workflows are disabled or fail, and still needs periodic recovery.

### 5. Separate snapshot freshness from content availability

Add synchronization health with `initializing`, `healthy`, and `degraded` states. It records the active revision, configured refs, last attempt, last success, safe failure category, and whether displayed content is last-known-good. The public health response never contains credentials, raw Git transport errors, local cache paths, or private-key details.

GitHub provenance is attached to existing source/change/spec response models through optional fields so filesystem clients remain compatible. It includes repository, canonical ref or pull-request number, commit SHA, and safe GitHub URLs. Accepted Specs always carry the configured canonical ref; a pull-request source never becomes accepted-spec authority merely because it is newer.

The UI keeps rendering the active snapshot during degradation, adds a visible stale-data warning and last-success time, and shows source provenance on cards and details. Before the first successful synchronization, it renders an initializing/error state instead of an empty board that could be mistaken for truth.

Alternative considered: clear the board on refresh failure. That destroys useful information and makes a transient upstream failure appear to mean there are no Changes or Specs.

### 6. Derive the archive warning from GitHub state without writing back

For each active change sourced from the canonical base, synchronization records the associated merged pull request when GitHub can identify it. If the change directory is still active 15 full days after `merged_at`, the API marks it as needing archive attention. The threshold is intentionally fixed at 15 days for this change. Open pull-request sources and already archived directories do not receive the warning.

The UI displays the warning and links to the merged pull request. It does not change task completion, automatically archive, or create a repository mutation. If association is unavailable, the service omits the warning rather than guessing from timestamps in Markdown files.

Alternative considered: use the last Markdown commit time. That does not prove a change was merged and can be altered by rebases or unrelated edits.

### 7. Ship one portable container contract behind an authenticated ingress

The production image includes the existing Rust/React application plus the Git client needed by the synchronizer, runs as a non-root user, mounts a writable cache directory, and exposes only the application port. Deployment documentation defines health checks, cache persistence, secret injection, restart behavior, and reverse-proxy requirements.

TLS and viewer authentication are required at the ingress because this change does not implement application login. The webhook path may bypass interactive viewer authentication but must remain protected by signature verification, request limits, and ingress rate limiting. Outbound network policy can be restricted to GitHub endpoints.

Alternative considered: bundling application authentication now. It would materially expand identity, session, and authorization scope without improving GitHub source correctness.

## Risks / Trade-offs

- [A compromised or malicious pull request supplies hostile paths, symlinks, or oversized documents] → Extract only allowed `openspec/` paths, reject traversal and escaping symlinks, enforce per-file and total snapshot limits, and parse without executing repository content.
- [GitHub is unavailable or rate limited] → Use conditional/ref-aware fetches, coalesce refreshes, retain last-known-good data, expose degraded health, and retry through bounded backoff plus periodic reconciliation.
- [A webhook is replayed or reordered] → Validate HMAC, persist delivery IDs, serialize repository refreshes, and publish only state resolved from current GitHub refs rather than trusting payload content.
- [The cache volume is lost] → Rebuild from GitHub on startup; the UI remains initializing until the first complete generation succeeds.
- [A repository has many open pull requests] → Filter by configured base branches, cap concurrent transfers and configured maximum PR sources, and surface an explicit degraded reason rather than silently truncating.
- [External ingress authentication is misconfigured] → Bind conservatively by default, document the required private authenticated ingress, and expose no repository write capability; deployment review remains an operator responsibility.
- [Filesystem and hosted behavior drift] → Share parser and response mapping tests across providers and retain the current filesystem tests as compatibility gates.

## Migration Plan

1. Add the provider-aware configuration and snapshot abstractions while retaining filesystem mode as the default.
2. Add GitHub App authentication, snapshot staging/publication, synchronization health, webhook ingestion, and reconciliation behind the GitHub provider selection.
3. Extend API and UI models with optional provenance, health, and archive-warning fields; verify filesystem clients still receive compatible responses.
4. Build the container, mount a persistent cache, inject GitHub App and webhook secrets, and deploy behind TLS plus private viewer authentication.
5. Install the GitHub App on the configured repository, register the signed webhook, then enable GitHub mode and wait for an initial healthy snapshot before directing viewers to the instance.
6. Roll back by routing viewers away, disabling GitHub mode, and deploying the previous image. Cache generations are disposable and can be removed after rollback because GitHub remains authoritative.

## Open Questions

- The first deployment target and ingress product can be selected during deployment preparation; the container, secret, health-check, and cache contracts do not depend on that choice.
