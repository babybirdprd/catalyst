//! # Pipeline Stages
//!
//! Defines the stages of the agent pipeline.

use serde::{Deserialize, Serialize};

/// Stage of the pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    /// Parsing user goal for unknowns
    UnknownsParsing,
    /// Researching solutions for unknowns
    Researching,
    /// Architect making decisions
    Architecting,
    /// Critic reviewing decisions
    Critiquing,
    /// Atomizer breaking down features
    Atomizing,
    /// Taskmaster generating mission prompts
    TaskGeneration,
    /// Complete
    Complete,
    /// Failed
    Failed,
}

/// The pipeline state machine
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Current stage
    pub stage: PipelineStage,
    /// Number of critic rejections (for loop detection)
    pub critic_rejections: u32,
    /// Maximum critic rejections before failing
    pub max_rejections: u32,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            stage: PipelineStage::UnknownsParsing,
            critic_rejections: 0,
            max_rejections: 3,
        }
    }
}

impl Pipeline {
    /// Create a new pipeline
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance to the next stage
    pub fn advance(&mut self) {
        self.stage = match self.stage {
            PipelineStage::UnknownsParsing => PipelineStage::Researching,
            PipelineStage::Researching => PipelineStage::Architecting,
            PipelineStage::Architecting => PipelineStage::Critiquing,
            PipelineStage::Critiquing => PipelineStage::Atomizing,
            PipelineStage::Atomizing => PipelineStage::TaskGeneration,
            PipelineStage::TaskGeneration => PipelineStage::Complete,
            PipelineStage::Complete => PipelineStage::Complete,
            PipelineStage::Failed => PipelineStage::Failed,
        };
    }

    /// Handle critic rejection - loop back to architect
    pub fn reject(&mut self) -> bool {
        self.critic_rejections += 1;
        if self.critic_rejections >= self.max_rejections {
            self.stage = PipelineStage::Failed;
            false
        } else {
            self.stage = PipelineStage::Architecting;
            true
        }
    }

    /// Fail the pipeline
    pub fn fail(&mut self) {
        self.stage = PipelineStage::Failed;
    }

    /// Check if pipeline is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.stage, PipelineStage::Complete | PipelineStage::Failed)
    }

    /// Check if pipeline succeeded
    pub fn is_success(&self) -> bool {
        self.stage == PipelineStage::Complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_advance() {
        let mut pipeline = Pipeline::new();
        assert_eq!(pipeline.stage, PipelineStage::UnknownsParsing);

        pipeline.advance();
        assert_eq!(pipeline.stage, PipelineStage::Researching);

        pipeline.advance();
        assert_eq!(pipeline.stage, PipelineStage::Architecting);
    }

    #[test]
    fn test_critic_rejection_loop() {
        let mut pipeline = Pipeline::new();
        pipeline.stage = PipelineStage::Critiquing;

        // First rejection - loop back
        assert!(pipeline.reject());
        assert_eq!(pipeline.stage, PipelineStage::Architecting);

        // Advance to critiquing again
        pipeline.stage = PipelineStage::Critiquing;

        // Second rejection
        assert!(pipeline.reject());
        assert_eq!(pipeline.stage, PipelineStage::Architecting);

        // Third rejection - fail
        pipeline.stage = PipelineStage::Critiquing;
        assert!(!pipeline.reject());
        assert_eq!(pipeline.stage, PipelineStage::Failed);
    }
}
