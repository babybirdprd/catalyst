# Catalyst

> **"One Brain, Two Bodies"**
> An Agent-First Architecture for Recursive Intelligence.

Catalyst is an opinionated, Rust-based agent framework that utilizes "Compile-Time Intelligence" to resolve ambiguities before coding. It treats agents not as junior developers, but as compilers of intent.

## ✨ Key Features

- **10 Specialized Agents** - Unknowns Parser, Researcher, Architect, Critic, Atomizer, Taskmaster, Builder, WebScraper, Gardener, Red Team
- **Brownfield Support** - `catalyst init` onboards existing codebases
- **Worktree Isolation** - Features build in git worktree sandboxes
- **Real-Time UI** - React dashboard with Pipeline, Factory, Memory views
- **Local-First Search** - Agents search project context before external APIs

## 🏗 Architecture

See [architecture.md](harness/specs/architecture.md) for the full deep dive.

The system is split into a central "Brain" (Core logic) and multiple "Bodies" (Runtime environments).

- **The Brain**: `crates/core` containing the Radkit Swarm, State, and Business Logic.
- **Body A (Server)**: `crates/server` (Axum) for web deployment.
- **Body B (Desktop)**: `apps/desktop` (Tauri) for native OS integration.
- **The Face**: `apps/frontend` (React + Vite) serving as the UI for both bodies.

## 📁 Project Structure

```
catalyst-core/
├── apps/
│   ├── desktop/        # Tauri App (Native Shell)
│   └── frontend/       # React Dashboard (Vite + Zustand)
├── crates/
│   ├── core/           # The Brain (Agents, Swarm, State, Tools)
│   └── server/         # The Server Body (Axum + PTY)
├── harness/            # The Self-Hosted Brain Context
│   ├── specs/          # Architecture, Constraints, Decisions
│   └── prompts/        # Agent System Prompts
├── .catalyst/          # Runtime State (created per-project)
│   ├── context/        # Braindump files + codebase_profile.json
│   └── features/       # Sharded feature state
└── ROADMAP.md          # Implementation Status
```

## 🚀 Quick Start

### Prerequisites
- Rust (latest stable)
- Node.js & pnpm (for frontend)
- `ANTHROPIC_API_KEY` environment variable

### Running the Server

```bash
# 1. Build the frontend
cd apps/frontend
pnpm install && pnpm build

# 2. Run the server
cd ../../
cargo run -p catalyst_server
```

Open http://localhost:8080

### Development Mode

```bash
cargo run -p catalyst_server -- --dev
```
This spawns Vite dev server for hot reload.

### Brownfield: Add to Existing Project

```bash
cd your-rust-project
# Start Catalyst server, onboarding UI will appear
cargo run -p catalyst_server
```

## 🤖 Agents

| Agent | Role |
|-------|------|
| **Unknowns Parser** | Extracts ambiguities from user goals |
| **Researcher** | Searches local context, crates.io, SearXNG |
| **Architect** | Designs solutions, creates ADRs |
| **Critic** | Reviews decisions, applies fortress rules |
| **Atomizer** | Breaks features into atomic modules |
| **Taskmaster** | Bundles context into mission prompts |
| **Builder** | Implements code in worktree sandbox |
| **WebScraper** | Cleans HTML for content extraction |
| **Gardener** | Fallback for error recovery |
| **Red Team** | Security analysis (planned) |

## 📊 Status

**Current Phase**: v1 Complete

| Component | Status |
|-----------|--------|
| Safety Layer (worktrees, merge, terminal) | ✅ |
| Data Layer (features, context, state) | ✅ |
| All 10 Agents | ✅ |
| Swarm Orchestration | ✅ |
| React Dashboard | ✅ |
| Brownfield Init + Profile Injection | ✅ |
| Real Search APIs (crates.io, SearXNG) | ✅ |

*See [ROADMAP.md](ROADMAP.md) for detailed progress.*

## 📚 Documentation

- [Architecture](harness/specs/architecture.md)
- [Decisions (ADRs)](harness/specs/decisions.md)
- [Constraints](harness/specs/constraints.md)
- [Brownfield Protocol](BROWNFIELD.md)
- [Manifesto](MANIFESTO.md)

## License

MIT
