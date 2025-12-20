//! # Catalyst Core
//!
//! The "Brain" of the Catalyst system - contains all business logic,
//! skill implementations, and state management.
//!
//! ## Architecture
//!
//! - `skills/` - A2A-native skills (ParseSkill, ResearcherSkill, ArchitectSkill, etc.)
//! - `models/` - Centralized LLM provider configuration
//! - `state/` - Read/write operations for spec.md and state.json
//! - `swarm/` - Agent orchestration and pipeline management
//!
//! ## Usage
//!
//! ```rust,ignore
//! use catalyst_core::swarm::{Coordinator, CoordinatorConfig};
//!
//! let config = CoordinatorConfig::default();
//! let mut coordinator = Coordinator::new(config);
//! let result = coordinator.run("Build a stock tracker").await?;
//! ```

pub mod memory;
pub mod models;
pub mod skills;
pub mod state;
pub mod swarm;
pub mod tools;

/// Initialize the Catalyst runtime
pub fn ignite() {
    println!("🚀 Catalyst Core Online");
}
