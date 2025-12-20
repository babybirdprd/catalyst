# Skills and Agents

Catalyst leverages the `radkit` framework to define agents. In our architecture, the distinction between a "Skill" and an "Agent" is important:

*   **Skill**: A reusable unit of logic (a Rust struct implementing `radkit::Skill`). It contains the prompt templates, tool definitions, and execution logic.
*   **Agent**: A container that holds one or more skills and a model configuration. Agents are the entities that the Coordinator interacts with.

## Defining Agents

Agents are defined in `crates/core/src/skills/agent_definitions.rs`. We use a builder pattern to compose them:

```rust
pub fn researcher_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Researcher")
        .with_skill(ResearcherSkill::new(config.clone()))
        .with_skill(WebScraperSkill::new(cheap_config))
        .build()
}
```

## Agent Roles

### 1. Unknowns Parser (`ParseSkill`)
*   **Phase**: Compile-Time
*   **Input**: User Goal (natural language)
*   **Output**: List of `Ambiguity` objects (questions that need answers).
*   **Role**: Prevents assumptions by forcing the system to ask clarifying questions before planning begins.

### 2. Researcher (`ResearcherSkill` + `WebScraperSkill`)
*   **Phase**: Compile-Time
*   **Input**: `Ambiguity` (question)
*   **Output**: `ResearchOutput` (summary of findings).
*   **Role**: The "Librarian". It uses search tools and web scraping to find documentation, libraries, and best practices. It does *not* make decisions; it provides data.

### 3. Architect (`ArchitectSkill`)
*   **Phase**: Compile-Time
*   **Input**: `ResearchOutput` + Project Context
*   **Output**: `ArchitectOutput` (chosen option, rationale, and updated spec).
*   **Role**: The "Decision Maker". It synthesizes research into a concrete plan. It chooses libraries, patterns, and structures.

### 4. Critic (`CriticSkill`)
*   **Phase**: Compile-Time
*   **Input**: `ArchitectOutput`
*   **Output**: `CriticOutput` (Verdict: Approve/Reject).
*   **Role**: The "Gatekeeper". It reviews the Architect's decisions against best practices, security constraints, and project requirements.

### 5. Atomizer (`AtomizerSkill`)
*   **Phase**: Compile-Time
*   **Input**: Approved Architecture
*   **Output**: List of `Module` definitions.
*   **Role**: Enforces the **Rule of 100**. It breaks large features into small, self-contained modules that fit within a single agent's context window.

### 6. Taskmaster (`TaskmasterSkill`)
*   **Phase**: Runtime Transition
*   **Input**: `Module` definition
*   **Output**: Mission Prompt.
*   **Role**: The "Manager". It bundles all necessary context (file paths, constraints, dependencies) into a highly specific prompt for the Builder.

### 7. Builder (`BuilderSkill`)
*   **Phase**: Runtime
*   **Input**: Mission Prompt
*   **Output**: Code changes (in a worktree).
*   **Role**: The "Worker". It has access to the file system and build tools. It writes code, runs `cargo build`, fixes errors, and requests review.

## Creating a New Skill

To add a new skill to Catalyst:

1.  Create a new file in `crates/core/src/skills/`.
2.  Define a struct that holds the `ModelConfig`.
3.  Implement `radkit::Skill` (or use the helper macros).
4.  Define the prompt template.
5.  Register the skill in `agent_definitions.rs`.

```rust
pub struct MyNewSkill {
    config: ModelConfig,
}

impl MyNewSkill {
    pub async fn run(&self, input: &str) -> Result<MyOutput> {
        // ... implementation using self.config.provider ...
    }
}
```
