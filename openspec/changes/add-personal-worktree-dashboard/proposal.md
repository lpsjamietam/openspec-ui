# Change: Add a safe personal worktree dashboard

## Why
Developers who use many Git worktrees need OpenSpec visibility without exposing specifications on the network, mutating repositories, or confusing copied changes with separate work. The dashboard should remain useful when newer OpenSpec CLI versions know more about an artifact workflow than the dashboard itself.

## What Changes
- Default the server to loopback and expose an explicit bind-address override.
- Add a true read-only mode that disables all idea and configuration mutations and is visible in the UI.
- Prefer artifact state reported by the installed OpenSpec CLI, with a transparent filesystem fallback.
- Detect Git branch, commit, detached-worktree state, configured delivery track, and target branch for each source.
- Collapse identical copies of a change across worktrees while preserving the list of contributing sources.

## Impact
- Affected specs: `api`, `config`, `ui`
- Affected code: Rust configuration/parser/API startup, React API types and dashboard cards, tests, README and example configuration
- Compatibility: existing configurations remain valid; safe defaults apply to omitted fields
