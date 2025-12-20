//! # Swarm Events
//!
//! Event types for agent-to-agent communication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Kind of swarm event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwarmEventKind {
    /// Pipeline started
    PipelineStarted,
    /// Agent started working
    AgentStarted,
    /// Agent completed successfully  
    AgentCompleted,
    /// Agent failed
    AgentFailed,
    /// Data passed between agents
    DataPassed,
    /// Critic rejected, looping back
    CriticRejected,
    /// Pipeline completed
    PipelineCompleted,
    /// Pipeline failed
    PipelineFailed,
    // === Research-specific events for A2A bridge ===
    /// Research mission started (async)
    ResearchStarted,
    /// Research progress update (searching, fetching, etc.)
    ResearchProgress,
    /// Research mission completed
    ResearchCompleted,
    // === Inbox interaction events ===
    /// UI should show inbox modal/badge - coordinator is blocked
    InteractionRequired,
    /// Acknowledgment that interaction was resolved
    InteractionResolved,
    // === State restoration events ===
    /// State was restored from a snapshot (rollback)
    StateRestored,
    // === Speed Demon drafting events ===
    /// Parallel drafting phase started (with total file count)
    DraftingStarted,
    /// Individual file drafted (progress update)
    DraftingProgress,
    /// All files drafted, ready for bulk write
    DraftingCompleted,
}

/// An event in the swarm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmEvent {
    /// Unique event ID
    pub id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Kind of event
    pub kind: SwarmEventKind,
    /// Agent that produced this event
    pub agent: String,
    /// Associated data (JSON)
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    /// Related unknown ID if applicable
    #[serde(default)]
    pub unknown_id: Option<String>,
}

impl SwarmEvent {
    /// Create a new event
    pub fn new(kind: SwarmEventKind, agent: &str) -> Self {
        Self {
            id: uuid_v4(),
            timestamp: Utc::now(),
            kind,
            agent: agent.to_string(),
            data: None,
            unknown_id: None,
        }
    }

    /// Add data to the event
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Add unknown ID to the event
    pub fn with_unknown(mut self, unknown_id: &str) -> Self {
        self.unknown_id = Some(unknown_id.to_string());
        self
    }
}

/// Generate a simple UUID v4
fn uuid_v4() -> String {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    format!("{:x}-{:x}", nanos, rand_u32())
}

/// Simple random number (not cryptographic)
fn rand_u32() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = SwarmEvent::new(SwarmEventKind::AgentStarted, "unknowns_parser")
            .with_unknown("UNK-001");

        assert_eq!(event.agent, "unknowns_parser");
        assert_eq!(event.unknown_id, Some("UNK-001".to_string()));
    }
}
