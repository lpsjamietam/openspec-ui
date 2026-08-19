# GitHub server mode

GitHub server mode runs OpenSpec UI as a read-only hosted dashboard. It builds one immutable snapshot from committed GitHub content: accepted Specs come only from `github.specsRef`; active Changes come from `github.changesBaseRef` and eligible open pull requests. Local worktrees and unpublished files remain a desktop-only concern.

## GitHub App setup

Create a GitHub App for the repository with these repository permissions:

- **Contents: Read-only**
- **Pull requests: Read-only**

Subscribe its webhook to:

- **Push**
- **Pull request**
- **Installation repositories**

Install the App on the configured repository. OpenSpec UI requests an installation token narrowed to that repository and the two read-only permissions.

Copy `openspec-ui.github.example.json` to your deployment configuration and set:

- `repository` to `owner/name`.
- `specsRef` to the canonical accepted-Specs branch, such as `demo/main`.
- `changesBaseRef` to the branch whose active Changes should appear.
- `pullRequestTargets` to the base branches whose open PR Changes should appear.
- `cachePath` to a persistent, writable directory mounted only for the `openspec` user.

The default reconciliation interval is 900 seconds. Webhooks trigger a prompt refresh; periodic reconciliation is the fallback for missed, delayed, or irrelevant deliveries.

## Secrets

Provide all four values. They are deliberately absent from the JSON configuration and API responses.

| Environment variable | Meaning |
| --- | --- |
| `GITHUB_APP_ID` | GitHub App ID |
| `GITHUB_APP_INSTALLATION_ID` | Installation ID for the repository |
| `GITHUB_APP_PRIVATE_KEY` | PEM private key; `\\n` escapes are accepted |
| `GITHUB_WEBHOOK_SECRET` | Webhook signing secret |

Each value also supports a `_FILE` form, such as `GITHUB_APP_PRIVATE_KEY_FILE=/run/secrets/github-app.pem`. Prefer file-backed secrets in containers. Missing-secret diagnostics report variable names only; private keys, webhook secrets, and installation tokens are never returned by health or content APIs.

## Container deployment

The container runs as UID 1000 and writes hosted state only beneath the configured cache mount.

```bash
docker build -t openspec-ui .

docker run --rm \
  -p 127.0.0.1:3000:3000 \
  -e BIND_ADDRESS=0.0.0.0 \
  -e OPENSPEC_UI_CONFIG=/app/openspec-ui.github.json \
  -e GITHUB_APP_ID=123456 \
  -e GITHUB_APP_INSTALLATION_ID=23456789 \
  -e GITHUB_APP_PRIVATE_KEY_FILE=/run/secrets/github-app.pem \
  -e GITHUB_WEBHOOK_SECRET_FILE=/run/secrets/github-webhook \
  -v "$PWD/openspec-ui.github.json:/app/openspec-ui.github.json:ro" \
  -v openspec-ui-cache:/data/openspec-ui \
  -v "$PWD/github-app.pem:/run/secrets/github-app.pem:ro" \
  -v "$PWD/github-webhook:/run/secrets/github-webhook:ro" \
  openspec-ui
```

Keep the cache on persistent storage. On restart, the server validates and publishes the last complete cached snapshot before reconciling. A failed refresh never replaces that generation; the health endpoint enters `degraded` state and marks that last-known-good content is being served. When GitHub becomes available, a webhook or periodic reconciliation returns the service to `healthy`.

Initial startup without a valid cached generation shows an explicit initializing state until the first GitHub reconciliation completes. Readiness should require both `GET /api/health` returning `ok` and `GET /api/sync-health` reporting an active revision. Liveness can use `/api/health` alone.

## Ingress and network policy

Do not expose the application directly to the public internet. Keep the process bound to loopback by default, or bind it to the container network only behind an ingress that provides:

- TLS termination.
- Private access or authenticated access for every UI and API route.
- A narrow unauthenticated exception only for `POST /api/github/webhook`.
- Per-IP and global request-rate limits on that webhook route.
- A request-size limit no larger than the application's 1 MiB webhook limit.

The webhook exception is still authenticated cryptographically: OpenSpec UI requires the GitHub delivery ID, event name, and a valid HMAC-SHA256 signature before it accepts or queues a refresh. Duplicate delivery IDs are persisted and ignored.

If your platform supports egress policy, allow HTTPS only to GitHub's API endpoint (and the infrastructure needed for DNS and certificate validation). If `apiBaseUrl` is changed for GitHub Enterprise, restrict egress to that configured host instead.

## GitHub webhook URL

Configure the App webhook URL as:

```text
https://openspec.example.com/api/github/webhook
```

Pushes to the canonical refs, eligible pull-request lifecycle events (including retargeting), and repository installation changes schedule synchronization. Bursts are coalesced into a single worker, and the server emits one browser refresh only when the published content revision changes.

## Operational limits

- One hosted repository is configured per process.
- Only paths beneath `openspec/` are materialized; symlinks, unsafe paths, oversized files, and oversized snapshots are rejected.
- Accepted Specs are never taken from a PR head.
- Changes merged at least 15 full days ago remain visible with a warning until their canonical Change directory is archived; the server never archives automatically.
- Merged-PR association is bounded by GitHub API history and `maxPullRequests`, so very old unassociated Changes may have no archive warning.
- Hosted mode is always read-only. Idea and source mutations return HTTP 403.
