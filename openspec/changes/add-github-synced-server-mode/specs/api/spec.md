## ADDED Requirements

### Requirement: Verified GitHub webhook ingestion
The API MUST expose a GitHub webhook endpoint that validates the request signature before accepting an event, rejects replayed delivery identifiers, and acknowledges accepted events without waiting for repository synchronization.

#### Scenario: Valid relevant delivery arrives
- **WHEN** a supported event has a valid signature and a delivery identifier not processed previously
- **THEN** the API records the delivery, enqueues repository reconciliation, and returns a successful acknowledgement promptly

#### Scenario: Signature is invalid
- **WHEN** a webhook signature is absent or fails constant-time validation
- **THEN** the API rejects the request and does not enqueue synchronization

#### Scenario: Delivery is replayed
- **WHEN** a previously accepted delivery identifier is received again
- **THEN** the API acknowledges it idempotently without scheduling duplicate work

#### Scenario: Event is irrelevant
- **WHEN** a valid event does not affect the configured repository or eligible refs
- **THEN** the API acknowledges it without scheduling synchronization

### Requirement: Synchronization health API
The API MUST report the active snapshot revision, contributing refs, last successful synchronization time, current synchronization state, and last failure without exposing secrets.

#### Scenario: Hosted snapshot is healthy
- **WHEN** the last reconciliation completed successfully
- **THEN** the health response identifies the canonical ref revision, eligible pull-request revisions, and successful synchronization time

#### Scenario: Last refresh failed
- **WHEN** the application is serving a last-known-good snapshot after a synchronization failure
- **THEN** the health response reports degraded state, failure time, and a safe error summary while ordinary read APIs continue serving the retained snapshot

### Requirement: Snapshot publication event
The event stream MUST notify connected clients after a new hosted snapshot becomes active and MUST NOT notify them for a failed or no-op reconciliation.

#### Scenario: Snapshot revision changes
- **WHEN** successful reconciliation atomically publishes different GitHub content
- **THEN** connected clients receive one update event and re-fetch the affected APIs

#### Scenario: Reconciliation finds no changes
- **WHEN** fetched revisions and parsed content match the active snapshot
- **THEN** the application does not emit a redundant update event
