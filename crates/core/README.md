# Catalyst Core (`catalyst_core`)

The "Brain" of the Catalyst system. This crate contains all business logic, agent implementations, state management, and orchestration tools. It is built on top of the `radkit` agent framework.

## Architecture

The core is organized into several key modules that work together to provide the Catalyst intelligence:

### 1. Skills (`skills/`)
The **Skills** module contains the implementation of all `radkit` Skills, which are the fundamental building blocks of agents. Catalyst uses an **Agent-to-Agent (A2A)** architecture where specialized agents collaborate to solve problems.

**Key Skills:**
*   **ParseSkill**: The "Unknowns Parser". Analyzes user goals to identify ambiguities before any code is written. (Compile-Time Intelligence)
*   **ResearcherSkill**: Searches external sources (docs, web) to answer unknowns.
*   **ArchitectSkill**: Makes high-level design decisions based on research.
*   **CriticSkill**: Reviews decisions and code for quality and correctness.
*   **AtomizerSkill**: Breaks large features into small, agent-sized modules (Rule of 100).
*   **TaskmasterSkill**: Generates detailed mission prompts for the Builder.
*   **BuilderSkill**: The runtime agent that writes code, runs builds, and fixes errors.

### 2. Swarm (`swarm/`)
The **Swarm** module handles the orchestration of agents. The `Coordinator` is the central nervous system that manages the pipeline from user goal to completed feature.

**Key Components:**
*   **Coordinator**: Manages the lifecycle of a request, dispatching tasks to agents in the correct order (Parse -> Research -> Architect -> Critic -> Atomize -> Build).
*   **Pipeline**: Defines the state transitions and flow of the agentic process.
*   **A2A Bridge**: Facilitates asynchronous communication between agents (e.g., parallel research tasks).

### 3. State (`state/`)
The **State** module manages the persistence of the project's state. Catalyst uses a hybrid approach:
*   **SQLite (`CatalystDb`)**: For structured data like features, interactions, and agent history.
*   **Markdown (`spec.md`)**: For human-readable specifications and documentation.
*   **JSON (`state.json`)**: For project configuration and metadata.

**Key Managers:**
*   `SpecManager`: Reads/writes the `spec.md` file.
*   `FeatureManager`: Manages the lifecycle of individual features (sharded state).
*   `InteractionManager`: Handles human-in-the-loop interactions (approvals, questions).

### 4. Tools (`tools/`)
The **Tools** module contains "Cyborg" tools—deterministic Rust code that replaces or wraps agent guesswork with strict compile-time guarantees.

**Key Tools:**
*   **Git**: Handles worktree isolation (each feature runs in a separate worktree) and merging.
*   **Terminal**: A wrapper around `cargo` and shell commands with structured output parsing.
*   **Scanner/AST**: Tools for analyzing the codebase structure (using `syn` and `tree-sitter`).
*   **Search**: Semantic search capabilities.

### 5. Memory (`memory/`)
The **Memory** module provides semantic memory for agents, allowing them to recall past conversations and knowledge. It integrates with `radkit`'s memory system and supports SQLite-backed persistence.

## Key Concepts

### Compile-Time Intelligence
Catalyst separates the "thinking" (planning) from the "doing" (coding).
1.  **Compile-Time**: Parse -> Research -> Architect -> Critic -> Atomize. This phase resolves all ambiguities and creates a perfect plan *before* touching the code.
2.  **Runtime**: Taskmaster -> Builder. This phase executes the plan with deterministic tools.

### Worktree Isolation
To prevent agents from breaking the main branch, every feature is developed in a temporary Git worktree. The `Builder` agent works in isolation, and the changes are only merged back after passing tests and review.

### 3-Truth Synthesis
When merging code, Catalyst compares:
1.  The Original Source
2.  The Spec (The Intent)
3.  The New Implementation
This ensures that merges respect both the code structure and the architectural intent.

## Usage

The core is typically consumed by the `catalyst_server` or the CLI.

```rust
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
