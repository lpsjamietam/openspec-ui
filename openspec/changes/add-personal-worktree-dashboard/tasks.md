## 1. Backend safety and configuration
- [x] 1.1 Add safe configuration defaults and source track/target metadata in `backend/src/config.rs`.
- [x] 1.2 Enforce read-only mode on mutating routes and bind to the configured loopback address in `backend/src/main.rs`.
- [x] 1.3 Add configuration and API tests for safe defaults and mutation rejection.

## 2. Worktree-aware status
- [x] 2.1 Add Git context discovery for configured sources.
- [x] 2.2 Add OpenSpec CLI status enrichment with explicit filesystem fallback.
- [x] 2.3 Add content-based duplicate grouping before CLI enrichment.
- [x] 2.4 Cover Git discovery, CLI mapping including skipped artifacts, fallback, and deduplication with Rust tests.

## 3. Dashboard experience
- [x] 3.1 Extend frontend contracts and hooks with runtime capabilities, Git context, track/target metadata, duplicate sources, and status provenance.
- [x] 3.2 Hide mutation controls in read-only mode and display the mode in the header/settings UI.
- [x] 3.3 Show branch, target, track, duplicate count, and fallback state on change cards and details.
- [x] 3.4 Update component tests for read-only behavior and worktree metadata.

## 4. Documentation and verification
- [x] 4.1 Update `README.md` and `openspec-ui.example.json` with safe personal-worktree configuration and limitations.
- [x] 4.2 Run strict OpenSpec validation, Rust format/clippy/tests, and frontend lint/tests/build.
- [x] 4.3 Exercise the binary against a real OpenSpec 1.9 repository and verify loopback/read-only behavior.

## 5. Canonical Specs source
- [x] 5.1 Add optional `specsSourceId` configuration and expose it through the config API.
- [x] 5.2 Restrict Specs list and detail endpoints to the configured source, preserving multi-source behavior when omitted.
- [x] 5.3 Cover configured and backward-compatible behavior with Rust tests.
- [x] 5.4 Document per-machine filesystem configuration and fresh-clone limitations.
- [x] 5.5 Run strict OpenSpec validation, Rust checks, frontend checks, rebuild the graph, and verify the running dashboard against `demo/main`.
