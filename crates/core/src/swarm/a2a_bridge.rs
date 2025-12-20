//! # A2A Bridge
//!
//! In-process bridge for running agents asynchronously with progress streaming.
//! Enables non-blocking research while keeping the Control Plane (Architect, Critic) synchronous.
//!
//! ## Architecture
//!
//! ```text
//! Coordinator                       Research Agent Task
//!     │                                    │
//!     ├─── ResearchMission ──────────────▶ │
//!     │                                    ├── search crates.io
//!     │ ◀──── ResearchProgress ────────────┤
//!     │                                    ├── fetch GitHub README
//!     │ ◀──── ResearchProgress ────────────┤
//!     │                                    ├── synthesize findings
//!     │ ◀──── ResearchOutput (oneshot) ────┘
//!     │
//!     └── Continue to Architect/Critic
//! ```

use crate::models::ModelConfig;
use crate::skills::researcher_skill::{ResearchOutput, ResearcherSkill};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// A research mission to be executed asynchronously
#[derive(Debug)]
pub struct ResearchMission {
    /// ID of the unknown being researched
    pub unknown_id: String,
    /// The question to research
    pub question: String,
    /// Additional context
    pub context: String,
    /// Channel to send the result back
    pub response_tx: oneshot::Sender<Result<ResearchOutput, anyhow::Error>>,
}

/// Progress updates from the research agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResearchProgress {
    /// Research started for an unknown
    Started { unknown_id: String },
    /// Intermediate status update
    Status { unknown_id: String, message: String },
    /// Research completed successfully
    Completed { unknown_id: String },
    /// Research failed
    Failed { unknown_id: String, error: String },
}

/// Handle for managing the research agent
pub struct ResearchAgentHandle {
    /// Channel to send missions
    pub mission_tx: mpsc::Sender<ResearchMission>,
    /// Handle to the spawned task
    pub task_handle: JoinHandle<()>,
}

/// Spawn a research agent that processes missions asynchronously
///
/// Returns channels for submitting missions and receiving progress updates.
pub fn spawn_research_agent(
    config: ModelConfig,
    progress_tx: mpsc::Sender<ResearchProgress>,
) -> ResearchAgentHandle {
    let (mission_tx, mut mission_rx) = mpsc::channel::<ResearchMission>(32);

    let task_handle = tokio::spawn(async move {
        while let Some(mission) = mission_rx.recv().await {
            let unknown_id = mission.unknown_id.clone();

            // Emit started event
            let _ = progress_tx
                .send(ResearchProgress::Started {
                    unknown_id: unknown_id.clone(),
                })
                .await;

            // Emit status: searching
            let _ = progress_tx
                .send(ResearchProgress::Status {
                    unknown_id: unknown_id.clone(),
                    message: "Searching crates.io and web...".to_string(),
                })
                .await;

            // Run the actual research
            let result = ResearcherSkill::run(
                &mission.unknown_id,
                &mission.question,
                &mission.context,
                &config,
            )
            .await;

            // Emit completion or failure event
            match &result {
                Ok(_) => {
                    let _ = progress_tx
                        .send(ResearchProgress::Completed {
                            unknown_id: unknown_id.clone(),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = progress_tx
                        .send(ResearchProgress::Failed {
                            unknown_id: unknown_id.clone(),
                            error: e.to_string(),
                        })
                        .await;
                }
            }

            // Send result back to coordinator
            let _ = mission.response_tx.send(result);
        }
    });

    ResearchAgentHandle {
        mission_tx,
        task_handle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_research_progress_serialization() {
        let progress = ResearchProgress::Started {
            unknown_id: "UNK-001".to_string(),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"type\":\"started\""));
        assert!(json.contains("UNK-001"));
    }

    #[tokio::test]
    async fn test_research_progress_status() {
        let progress = ResearchProgress::Status {
            unknown_id: "UNK-002".to_string(),
            message: "Searching...".to_string(),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"type\":\"status\""));
        assert!(json.contains("Searching..."));
    }
}
