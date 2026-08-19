# OpenSpec UI — a web dashboard for OpenSpec

[![CI](https://github.com/ToruAI/openspec-ui/actions/workflows/ci.yml/badge.svg)](https://github.com/ToruAI/openspec-ui/actions/workflows/ci.yml)
[![OpenSpec 1.9](https://img.shields.io/badge/OpenSpec-1.9%20compatible-blue)](https://github.com/Fission-AI/OpenSpec)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**A single kanban board over every [OpenSpec](https://github.com/Fission-AI/OpenSpec) repo you work in** — which change is where in the workflow, what is ready to write next, and what is still blocked. `openspec view` shows you one repo in the terminal; this shows all of them, in a browser, on your phone too.

Read-only by default, loopback-only, and one binary. Writable idea capture remains available as an explicit opt-in.

<p align="center">
  <img src="./desktop.png" alt="OpenSpec UI Desktop" width="600"/>
  <img src="./mobile.png" alt="OpenSpec UI Mobile" width="150"/>
</p>
<p align="center"><em>Desktop and mobile views — mobile-first design should work on any device</em></p>

## Why

AI coding assistants are powerful, but when you're working across multiple projects, it's hard to keep track of what's happening where. Which features are in progress? What did the agent finish yesterday? What ideas are waiting to be developed?

OpenSpec UI solves this by giving you a single dashboard to monitor all your AI-assisted projects — capture ideas on the go, watch progress in real-time, and stay in sync with your agents.

## What is OpenSpec?

[OpenSpec](https://github.com/Fission-AI/OpenSpec) is a spec-driven development (SDD) framework for AI coding assistants. Instead of jumping straight into code, you first define **what** should be built through structured proposals and specifications. Your AI agent then implements the spec, task by task.

```
openspec/
├── specs/          # What IS built (source of truth)
├── changes/        # What SHOULD change (proposals + tasks)
└── ideas/          # Quick thoughts to develop later
```

OpenSpec UI reads this structure and displays it as a kanban board.

## What It Does

OpenSpec UI gives you a bird's-eye view of all your AI-assisted projects:

- **Artifact chain per change** — see each workflow artifact as complete, ready, blocked, or skipped. OpenSpec CLI status is preferred, with a visible filesystem fallback.
- **Multi-repo visibility** — monitor every OpenSpec repository from one board
- **Worktree context** — see the current Git branch/commit plus optional delivery track and target branch
- **Duplicate grouping** — collapse identical copies of a change while keeping every contributing source visible
- **Capture ideas (opt-in)** — enable writable mode when you want the UI to create or edit idea files
- **Track progress** — watch changes move from Ideas → Todo → In Progress → Done
- **Real-time updates** — auto-refreshes as your agents work through tasks

Because the artifact states are derived from the files themselves, the dashboard needs no OpenSpec install in the repos it watches, and it works the same whether a change was created by you or by an agent.

## The Workflow

```
💡 Idea  →  📋 Proposal  →  ⚡ Implementation  →  ✅ Done
   ↑            ↑                  ↑
  You      AI Agent           AI Agent
```

1. **Capture an idea** in the UI
2. **Work with your AI agent** to expand it into a full OpenSpec proposal
3. **Watch progress** as the agent implements tasks
4. **Archive** completed changes

OpenSpec UI is the mission control — the actual spec-driven development happens through [OpenSpec](https://github.com/Fission-AI/OpenSpec) and your AI coding assistant (Cursor, Claude Code, etc.).

## Installation

### Option 1: Download Binary (Recommended)

Download the latest release from [GitHub Releases](https://github.com/ToruAI/openspec-ui/releases):

| Platform | File |
|----------|------|
| macOS (Apple Silicon) | `openspec-ui-<version>-darwin-aarch64.zip` |
| macOS (Intel) | `openspec-ui-<version>-darwin-x86_64.zip` |
| Linux (x86_64) | `openspec-ui-<version>-linux-x86_64.zip` |
| Windows (x86_64) | `openspec-ui-<version>-windows-x86_64.zip` |

```bash
# Extract and run (Linux/macOS)
unzip openspec-ui-*.zip
./openspec-ui --config openspec-ui.json
```

### Option 2: Docker

```bash
docker build -t openspec-ui .

docker run -p 127.0.0.1:3000:3000 \
  -e BIND_ADDRESS=0.0.0.0 \
  -v /path/to/your/repos:/repos:ro \
  -v /path/to/openspec-ui.json:/app/openspec-ui.json:ro \
  openspec-ui
```

The container must listen on `0.0.0.0` internally for Docker port forwarding; publishing the port to `127.0.0.1` keeps it local to your machine. Read-only mounts add a second safety boundary.

### Option 3: Build from Source

**Prerequisites:** Rust (stable), Node.js 18+

```bash
# Quick build & run (creates default config if missing)
./build_n_run.sh

# Or step by step:
cd frontend && npm ci && npm run build && cd ..
cd backend && cargo build --release && cd ..
./backend/target/release/openspec-ui --config openspec-ui.json
```

## Configuration

Create `openspec-ui.json`:

```json
{
  "sources": [
    {
      "name": "my-project-main",
      "path": "/path/to/my-project/openspec",
      "track": "production",
      "targetBranch": "main"
    },
    {
      "name": "my-project-demo",
      "path": "/path/to/my-project-worktree/openspec",
      "track": "demo",
      "targetBranch": "demo/main"
    }
  ],
  "specsSourceId": "my-project-demo",
  "port": 3000,
  "bindAddress": "127.0.0.1",
  "readOnly": true,
  "deduplicateChanges": true,
  "statusProvider": "auto",
  "openspecCommand": "openspec"
}
```

| Field | Description |
|-------|-------------|
| `sources` | Array of OpenSpec directories to monitor |
| `sources[].name` | Display name for the project |
| `sources[].path` | Path to the `openspec/` directory |
| `sources[].track` | Optional workflow label such as `demo` or `production` |
| `sources[].targetBranch` | Optional intended merge target such as `demo/main` or `main` |
| `specsSourceId` | Optional source name used exclusively by the Specs list and detail APIs; omit it to browse specs from every source |
| `port` | Server port (default: 3000) |
| `bindAddress` | Listener address (default: `127.0.0.1`) |
| `readOnly` | Reject all idea/config mutations and hide mutation controls (default: `true`) |
| `deduplicateChanges` | Group changes with the same name and content across worktrees (default: `true`) |
| `statusProvider` | `auto` tries the OpenSpec CLI, then falls back to files; `filesystem` never shells out (default: `auto`) |
| `openspecCommand` | OpenSpec executable used by `auto` mode (default: `openspec`) |

Git branch, short commit, detached-HEAD state, and worktree root are discovered automatically for each source. Deduplication only groups byte-identical change directories; divergent worktree copies remain separate. The representative card lists all grouped source IDs.

### Using the dashboard on another computer

OpenSpec UI reads the filesystem paths in that computer's configuration. Git transfers committed files, but it does not transfer another machine's uncommitted changes, worktrees, or absolute paths.

A fresh clone checked out on `main` can show only the OpenSpec content committed on `main`. To use `demo/main` as the canonical Specs source, fetch it into a local worktree and point the local configuration at that worktree:

```bash
git fetch origin demo/main
git worktree add ../my-project-demo origin/demo/main
```

Then set the matching source name in `specsSourceId`. Feature Changes appear only when their branches are committed and cloned/fetched locally, or when their local worktrees are configured on that machine.

### Writable mode

Set `"readOnly": false` only when you intentionally want the dashboard to create, edit, or delete ideas and update its source list. OpenSpec changes and specs remain display-only; the writable endpoints are limited to the existing idea and source-configuration operations.

## Features

- **Kanban Board** — Ideas, Todo, In Progress, Done, Archived columns
- **Artifact chain** — per change: written / ready to write / blocked, with what it is waiting for
- **Specs Browser** — Browse specifications from one configured canonical source, or across all sources when no source is selected
- **Detail View** — View proposals, specs, tasks, and design documents
- **Real-time Updates** — Auto-refreshes when files change (SSE)
- **Mobile-first** — Works great on phone and tablet
- **Light/Dark Theme** — Toggle between themes

## OpenSpec compatibility

Tested against **OpenSpec 1.9**, and still reads the pre-1.0 layout.

A change is recognised either by its `.openspec.yaml` marker (written by `openspec new change` from 1.0 onward) or by a `proposal.md` (older layout). This matters: OpenSpec creates the marker first and the proposal only once your agent drafts it, so a change that exists but has no proposal yet is still a change — and shows on the board as `proposal: ready` instead of being invisible.

With `statusProvider: "auto"`, artifact states come from `openspec status --change <name> --json`, so custom schemas and skipped artifacts are represented correctly. If the CLI is missing or returns invalid data, the card is explicitly marked `filesystem fallback` and uses the built-in `spec-driven` inference. Set `statusProvider: "filesystem"` for environments where spawning the CLI is undesirable.

## Tech Stack

- **Frontend**: React + TypeScript + Tailwind CSS + shadcn/ui
- **Backend**: Rust (Axum)
- **Real-time**: Server-Sent Events (SSE)

## Related

- [OpenSpec](https://github.com/Fission-AI/OpenSpec) — The spec-driven development framework this UI is built for

## License

MIT
