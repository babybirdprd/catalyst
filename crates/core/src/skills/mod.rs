//! # Catalyst Skills
//!
//! A2A-native skills and tools for the Catalyst swarm.
//!
//! ## Architecture
//!
//! ```text
//! Agent (A2A server)
//!   └── Skills (#[skill] + SkillHandler)
//!         └── Tools (#[tool] functions)
//! ```
//!
//! ## Skill Categories
//!
//! **Compile-Time Skills** (resolve unknowns BEFORE coding):
//! - `ParseSkill` - Identify ambiguities in user goals
//! - `ResearcherSkill` - Research solutions for unknowns
//! - `ArchitectSkill` - Make design decisions
//! - `CriticSkill` - Review and validate decisions
//! - `AtomizerSkill` - Break features into modules
//!
//! **Bridge:**
//! - `TaskmasterSkill` - Generate mission prompts for coding
//!
//! **Runtime Skills** (execute code generation):
//! - `BuilderSkill` - Implement features in code
//!
//! **Safety Layer Skills** (cyborg validation):
//! - `ConstraintSkill` - Validate code against Rule of 100
//! - `MergeSkill` - Resolve git merge conflicts
//!
//! **Utility Skills:**
//! - `WebScraperSkill` - Clean HTML for research

pub mod llm_helpers;
pub mod prompts;
pub mod tools;

// Artifact Registry (shared A2A artifact types)
pub mod artifact_registry;

// Compile-Time Skills
pub mod architect_skill;
pub mod atomizer_skill;
pub mod critic_skill;
pub mod parse_skill;
pub mod researcher_skill;

// Bridge
pub mod taskmaster_skill;

// Runtime Skills
pub mod builder_skill;
pub mod drafting_skill;

// Safety Layer Skills
pub mod constraint_skill;
pub mod merge_skill;

// Meta-Agent Skills
pub mod orchestrator_skill;

// Utility Skills
pub mod webscraper_skill;

// Agent Definitions (compose skills into agents)
pub mod agent_definitions;

// Re-exports for convenience
pub use architect_skill::ArchitectSkill;
pub use atomizer_skill::AtomizerSkill;
pub use builder_skill::BuilderSkill;
pub use constraint_skill::ConstraintSkill;
pub use critic_skill::CriticSkill;
pub use drafting_skill::{DraftingMission, DraftingOutput, DraftingSkill};
pub use merge_skill::MergeSkill;
pub use orchestrator_skill::OrchestratorSkill;
pub use parse_skill::ParseSkill;
pub use researcher_skill::ResearcherSkill;
pub use taskmaster_skill::TaskmasterSkill;
pub use webscraper_skill::WebScraperSkill;

// Agent factory functions
pub use agent_definitions::{
    architect_agent, atomizer_agent, builder_agent, create_compile_time_agents,
    create_runtime_agents, create_swarm, critic_agent, orchestrator_agent, researcher_agent,
    taskmaster_agent, unknowns_parser_agent,
};
