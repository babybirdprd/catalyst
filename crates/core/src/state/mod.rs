pub mod codebase_profile;
pub mod context_state;
pub mod db;
pub mod feature_state;
pub mod interaction;
pub mod io;
pub mod json;
pub mod snapshots;
pub mod specs;

pub use db::CatalystDb;

pub use codebase_profile::{CodebaseProfile, ProjectType, StylePatterns};
pub use context_state::{ContextFile, ContextManager, Idea};
pub use feature_state::{Feature, FeatureManager, PipelineStage};
pub use interaction::{
    Interaction, InteractionKind, InteractionManager, InteractionResponse, InteractionStatus,
};
pub use json::ProjectState;
pub use snapshots::{RollbackResult, Snapshot, SnapshotManager};
pub use specs::SpecManager;
