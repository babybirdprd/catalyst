# Catalyst Core (`catalyst_core`)

The "Brain" of the Catalyst system. This crate contains all business logic, agent implementations, state management, and orchestration tools. It is built on top of the `radkit` agent framework.

## 📚 Documentation

Detailed documentation for the core system is available in the `docs/` directory:

*   **[Architecture](./docs/ARCHITECTURE.md)**: Deep dive into the Swarm, Coordinator, Pipeline state machine, and A2A Bridge.
*   **[Skills & Agents](./docs/SKILLS_AND_AGENTS.md)**: Breakdown of each agent's role (Parser, Researcher, Architect, etc.) and how to build new Skills.
*   **[State Management](./docs/STATE_MANAGEMENT.md)**: Explanation of the hybrid state model (SQLite + JSON + Markdown).
*   **[Tools Reference](./docs/TOOLS_REFERENCE.md)**: Guide to the "Cyborg Tools" (Git worktrees, AST parsing, etc.).

## Quick Overview

### Architecture

The core is organized into several key modules that work together to provide the Catalyst intelligence:

*   `skills/`: A2A-native skills (ParseSkill, ResearcherSkill, ArchitectSkill, etc.)
*   `swarm/`: Agent orchestration and pipeline management
*   `state/`: Read/write operations for spec.md, state.json, and SQLite
*   `tools/`: Deterministic helpers (Git, Terminal, Scanner)
*   `memory/`: Semantic knowledge persistence

### Key Concepts

*   **Compile-Time Intelligence**: Resolving all unknowns and creating a perfect plan before writing a single line of code.
*   **Worktree Isolation**: Each feature is built in a temporary Git worktree to prevent breaking the main branch.
*   **Code > Agents**: Preferring deterministic Rust tools over LLM guesswork where possible.

## Usage

The core is typically consumed by the `catalyst_server` or the CLI.

```rust,ignore
use catalyst_core::swarm::{Coordinator, CoordinatorConfig};
use catalyst_core::state::CatalystDb;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize DB
    let db = Arc::new(CatalystDb::open("catalyst.db")?);

    // 2. Configure Coordinator
    let config = CoordinatorConfig::default();
    let mut coordinator = Coordinator::new(config, db);

    // 3. Run the swarm on a goal
    let result = coordinator.run("Build a stock tracking feature").await?;

    if result.success {
        println!("Feature built successfully!");
    } else {
        println!("Swarm failed.");
    }

    Ok(())
}
```

## Contributing

*   **Logic**: Place business logic in `src/skills` or `src/swarm`.
*   **State**: Update `src/state` for new data structures.
*   **Tools**: Add new deterministic tools to `src/tools`.
*   **Tests**: Write tests for all new skills and tools.
