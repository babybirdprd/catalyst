//! # A2A Agent Definitions
//!
//! Composes Catalyst agents from skills using radkit's Agent::builder().
//! Each agent can run as an A2A server exposing its skills.

use crate::models::ModelConfig;
use radkit::agent::{Agent, AgentDefinition};

use crate::skills::{
    ArchitectSkill, AtomizerSkill, BuilderSkill, CriticSkill, OrchestratorSkill, ParseSkill,
    ResearcherSkill, TaskmasterSkill, WebScraperSkill,
};

/// The Unknowns Parser Agent
///
/// First agent in the swarm. Identifies ambiguities before code generation.
/// Part of "Compile-Time Intelligence".
pub fn unknowns_parser_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Unknowns Parser")
        .with_description(
            "Parses user goals and identifies ambiguities that must be resolved \
             before code generation. First step in Compile-Time Intelligence.",
        )
        .with_skill(ParseSkill::new(config))
        .build()
}

/// The Researcher Agent
///
/// Second agent. Researches solutions for unknowns.
pub fn researcher_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Researcher")
        .with_description(
            "Researches solutions for technical unknowns. Searches crates.io \
             and the web for best practices and libraries.",
        )
        .with_skill(ResearcherSkill::new(config.clone()))
        .with_skill(WebScraperSkill::new(ModelConfig::new(
            "claude-3-haiku-20240307",
        ))) // Cheap model for scraping
        .build()
}

/// The Architect Agent
///
/// Third agent. Makes design decisions based on research.
pub fn architect_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Architect")
        .with_description(
            "Makes architectural decisions based on research options and project context. \
             Chooses the best approach and updates specifications.",
        )
        .with_skill(ArchitectSkill::new(config))
        .build()
}

/// The Critic Agent
///
/// Fourth agent. Reviews and validates decisions.
pub fn critic_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Critic")
        .with_description(
            "Reviews architectural decisions and code changes for quality and correctness. \
             Provides approval, rejection, or change requests.",
        )
        .with_skill(CriticSkill::new(config))
        .build()
}

/// The Atomizer Agent
///
/// Fifth agent. Breaks features into agent-sized modules.
pub fn atomizer_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Atomizer")
        .with_description(
            "Breaks features into agent-sized modules following the Rule of 100. \
             Each module is completable in one agent conversation.",
        )
        .with_skill(AtomizerSkill::new(config))
        .build()
}

/// The Taskmaster Agent
///
/// Sixth agent. Generates mission prompts for coding agents.
/// Bridge between compile-time planning and runtime execution.
pub fn taskmaster_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Taskmaster")
        .with_description(
            "Bundles context into mission prompts for coding agents. \
             The bridge from compile-time planning to runtime execution.",
        )
        .with_skill(TaskmasterSkill::new(config))
        .build()
}

/// The Builder Agent
///
/// Seventh agent. Implements features in code.
/// The main "Runtime" agent that writes code.
pub fn builder_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Builder")
        .with_description(
            "Implements features in code. Reads files, writes code, runs builds to verify. \
             The main Runtime agent that executes implementation.",
        )
        .with_skill(BuilderSkill::new(config))
        .build()
}

/// The Orchestrator Agent
///
/// Meta-agent that coordinates the entire pipeline.
/// Manages state transitions and delegates to specialized skills.
pub fn orchestrator_agent(config: ModelConfig) -> AgentDefinition {
    Agent::builder()
        .with_name("Orchestrator")
        .with_description(
            "Coordinates the agent pipeline from user goal to completed feature. \
             Manages state transitions: Parse → Research → Architect → Critic → Atomize → Build.",
        )
        .with_skill(OrchestratorSkill::new(config))
        .build()
}

/// Create the full Catalyst swarm (all agents)
///
/// Returns agents in pipeline order:
/// 0. Orchestrator (meta-coordinator)
/// 1. Unknowns Parser → 2. Researcher → 3. Architect → 4. Critic
/// → 5. Atomizer → 6. Taskmaster → 7. Builder
pub fn create_swarm(config: ModelConfig) -> Vec<AgentDefinition> {
    vec![
        orchestrator_agent(config.clone()),
        unknowns_parser_agent(config.clone()),
        researcher_agent(config.clone()),
        architect_agent(config.clone()),
        critic_agent(config.clone()),
        atomizer_agent(config.clone()),
        taskmaster_agent(config.clone()),
        builder_agent(config),
    ]
}

/// Create compile-time agents only (for resolution phase)
pub fn create_compile_time_agents(config: ModelConfig) -> Vec<AgentDefinition> {
    vec![
        unknowns_parser_agent(config.clone()),
        researcher_agent(config.clone()),
        architect_agent(config.clone()),
        critic_agent(config.clone()),
        atomizer_agent(config),
    ]
}

/// Create runtime agents only (for execution phase)
pub fn create_runtime_agents(config: ModelConfig) -> Vec<AgentDefinition> {
    vec![taskmaster_agent(config.clone()), builder_agent(config)]
}
