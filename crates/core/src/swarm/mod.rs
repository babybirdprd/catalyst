//! # Swarm Orchestration
//!
//! Coordinates the agent pipeline for Catalyst.
//!
//! ## Pipeline Flow
//!
//! ```text
//! User Goal → Unknowns Parser → Researcher → Architect ⟷ Critic → Atomizer → Taskmaster
//! ```

pub mod a2a_bridge;
pub mod architecture_generator;
pub mod coordinator;
pub mod events;
pub mod init;
pub mod pipeline;

pub use a2a_bridge::{
    spawn_research_agent, ResearchAgentHandle, ResearchMission, ResearchProgress,
};
pub use coordinator::{
    ApprovalRequest, ApprovalResponse, Coordinator, CoordinatorCommand, CoordinatorConfig,
};
pub use events::{SwarmEvent, SwarmEventKind};
pub use init::{detect_project, initialize_project, ScanProgress};
pub use pipeline::{Pipeline, PipelineStage};
