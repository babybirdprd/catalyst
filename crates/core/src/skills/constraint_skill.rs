//! # Constraint Skill
//!
//! A2A-native skill that validates code against project constraints.
//! Enforces the "Rule of 100" with human-in-the-loop approval for violations.
//!
//! ## Usage
//!
//! Input can be:
//! - Plain text file path: `src/lib.rs`
//! - JSON: `{ "file_path": "src/lib.rs" }` or `{ "code": "...", "file_name": "test.rs" }`
//!
//! ## Approval Flow
//!
//! When blocking violations are found, the skill returns `OnRequestResult::InputRequired`
//! asking for human approval to proceed. The user can approve or reject.

use crate::skills::artifact_registry::{ConstraintArtifact, ConstraintViolationSummary};
use crate::tools::linter::{
    scan_content, scan_file_with_config, ConstraintConfig, Violation, ViolationKind,
};
use async_trait::async_trait;
use radkit::agent::{Artifact, OnRequestResult, SkillHandler, SkillSlot};
use radkit::errors::{AgentError, AgentResult};
use radkit::macros::skill;
use radkit::models::Content;
use radkit::runtime::context::{ProgressSender, State};
use radkit::runtime::AgentRuntime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Types
// ============================================================================

/// Slot for tracking approval state in multi-turn conversations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalSlot {
    /// Waiting for user to approve constraint violations
    PendingConstraintApproval,
}

/// Saved state when awaiting approval
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingApproval {
    pub violations: Vec<ViolationInfo>,
    pub file_path: String,
}

/// Serializable violation info
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViolationInfo {
    pub kind: String,
    pub line: Option<u32>,
    pub message: String,
}

impl From<&Violation> for ViolationInfo {
    fn from(v: &Violation) -> Self {
        Self {
            kind: format!("{:?}", v.kind),
            line: v.line,
            message: v.message.clone(),
        }
    }
}

// ============================================================================
// Skill Definition
// ============================================================================

/// Constraint validation skill
#[skill(
    id = "constraint",
    name = "Constraint Validator",
    description = "Validates code against project constraints (Rule of 100). Checks file length, function length, and unwrap usage. Requires human approval to proceed with violations.",
    tags = ["safety", "validation", "linting", "cyborg"],
    examples = [
        "Validate code constraints for src/lib.rs",
        "Check function length limits",
        "Scan file for Rule of 100 violations"
    ],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct ConstraintSkill {
    config: ConstraintConfig,
}

impl ConstraintSkill {
    pub fn new(config: ConstraintConfig) -> Self {
        Self { config }
    }

    pub fn default() -> Self {
        Self::new(ConstraintConfig::default())
    }

    /// Create with custom limits
    pub fn with_limits(max_file_lines: usize, max_function_lines: usize) -> Self {
        Self::new(ConstraintConfig {
            max_file_lines,
            max_function_lines,
            ..Default::default()
        })
    }
}

#[async_trait]
impl SkillHandler for ConstraintSkill {
    async fn on_request(
        &self,
        state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let input = content.first_text().unwrap_or_default();

        progress
            .send_update("Scanning for constraint violations...")
            .await?;

        // Parse input - can be file path or JSON
        let (violations, file_path) = self.scan_input(input).await?;

        // Check for blocking violations (file/function too long)
        let blocking_violations: Vec<_> = violations
            .iter()
            .filter(|v| {
                matches!(
                    v.kind,
                    ViolationKind::FileTooLong | ViolationKind::FunctionTooLong
                )
            })
            .collect();

        if !blocking_violations.is_empty() {
            // Save state for multi-turn approval flow
            let pending = PendingApproval {
                violations: violations.iter().map(ViolationInfo::from).collect(),
                file_path: file_path.clone(),
            };
            state
                .task()
                .save("pending_approval", &pending)
                .map_err(|e| AgentError::Internal {
                    component: "constraint_skill".to_string(),
                    reason: format!("Failed to save state: {}", e),
                })?;
            state
                .set_slot(ApprovalSlot::PendingConstraintApproval)
                .map_err(|e| AgentError::Internal {
                    component: "constraint_skill".to_string(),
                    reason: format!("Failed to set slot: {}", e),
                })?;

            let summary = blocking_violations
                .iter()
                .map(|v| {
                    format!(
                        "- {}: {}",
                        v.line
                            .map(|l| format!("Line {}", l))
                            .unwrap_or_else(|| "File".to_string()),
                        v.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            progress
                .send_update("Violations found, awaiting approval...")
                .await?;

            return Ok(OnRequestResult::InputRequired {
                message: Content::from_text(&format!(
                    "⚠️ **Constraint Violations Found**\n\n\
                     File: `{}`\n\n\
                     {}\n\n\
                     Reply **approve** to proceed anyway, or **reject** to require fixes.",
                    file_path, summary
                )),
                slot: SkillSlot::new(ApprovalSlot::PendingConstraintApproval),
            });
        }

        // No blocking violations - create success artifact
        self.create_result_artifact(&violations, true, &file_path, progress)
            .await
    }

    async fn on_input_received(
        &self,
        state: &mut State,
        progress: &ProgressSender,
        _runtime: &dyn AgentRuntime,
        content: Content,
    ) -> AgentResult<radkit::agent::OnInputResult> {
        let input = content.first_text().unwrap_or_default().to_lowercase();

        // Load pending approval state
        let pending: Option<PendingApproval> =
            state
                .task()
                .load("pending_approval")
                .map_err(|e| AgentError::Internal {
                    component: "constraint_skill".to_string(),
                    reason: format!("Failed to load state: {}", e),
                })?;

        let pending = pending.ok_or_else(|| AgentError::Internal {
            component: "constraint_skill".to_string(),
            reason: "No pending approval found".to_string(),
        })?;

        // Check user response
        if input.contains("approve") || input.contains("yes") || input.contains("proceed") {
            state.clear_slot();
            progress
                .send_update("Approved - proceeding with violations...")
                .await?;

            // Create artifact with violations noted
            let artifact_data = ConstraintArtifact {
                passed: false,
                violations: pending
                    .violations
                    .iter()
                    .map(|v| ConstraintViolationSummary {
                        kind: v.kind.clone(),
                        location: pending.file_path.clone(),
                        actual: 0,
                        limit: 0,
                        message: v.message.clone(),
                    })
                    .collect(),
                warnings_count: 0,
                errors_count: pending.violations.len(),
            };

            let artifact =
                Artifact::from_json("constraint_result.json", &artifact_data).map_err(|e| {
                    AgentError::Internal {
                        component: "constraint_skill".to_string(),
                        reason: format!("Failed to create artifact: {}", e),
                    }
                })?;

            Ok(radkit::agent::OnInputResult::Completed {
                message: Some(Content::from_text(&format!(
                    "⚠️ Approved with {} violations in `{}`",
                    pending.violations.len(),
                    pending.file_path
                ))),
                artifacts: vec![artifact],
            })
        } else if input.contains("reject") || input.contains("no") || input.contains("fix") {
            state.clear_slot();
            progress
                .send_update("Rejected - changes will not proceed.")
                .await?;

            Ok(radkit::agent::OnInputResult::Failed {
                error: Content::from_text(&format!(
                    "Constraint violations must be fixed in `{}`",
                    pending.file_path
                )),
            })
        } else {
            // Unclear response, ask again
            Ok(radkit::agent::OnInputResult::InputRequired {
                message: Content::from_text(
                    "Please reply **approve** to proceed with violations, or **reject** to require fixes.",
                ),
                slot: SkillSlot::new(ApprovalSlot::PendingConstraintApproval),
            })
        }
    }
}

impl ConstraintSkill {
    /// Parse input and scan for violations
    async fn scan_input(&self, input: &str) -> AgentResult<(Vec<Violation>, String)> {
        // Try parsing as JSON first
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(input) {
            // Check for inline code
            if let Some(code) = parsed.get("code").and_then(|v| v.as_str()) {
                let file_name = parsed
                    .get("file_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown.rs");

                let violations = scan_content(code, file_name, &self.config).map_err(|e| {
                    AgentError::Internal {
                        component: "constraint_skill".to_string(),
                        reason: e.to_string(),
                    }
                })?;

                return Ok((violations, file_name.to_string()));
            }

            // Check for file path
            if let Some(file_path) = parsed.get("file_path").and_then(|v| v.as_str()) {
                let path = Path::new(file_path);
                let violations = scan_file_with_config(path, &self.config).map_err(|e| {
                    AgentError::Internal {
                        component: "constraint_skill".to_string(),
                        reason: e.to_string(),
                    }
                })?;

                return Ok((violations, file_path.to_string()));
            }
        }

        // Treat as file path
        let path = Path::new(input.trim());
        let violations =
            scan_file_with_config(path, &self.config).map_err(|e| AgentError::Internal {
                component: "constraint_skill".to_string(),
                reason: e.to_string(),
            })?;

        Ok((violations, input.trim().to_string()))
    }

    /// Create result artifact
    async fn create_result_artifact(
        &self,
        violations: &[Violation],
        passed: bool,
        file_path: &str,
        progress: &ProgressSender,
    ) -> AgentResult<OnRequestResult> {
        let warnings = violations
            .iter()
            .filter(|v| v.kind == ViolationKind::UnwrapUsed)
            .count();

        let artifact_data = ConstraintArtifact {
            passed,
            violations: violations
                .iter()
                .map(|v| ConstraintViolationSummary {
                    kind: format!("{:?}", v.kind),
                    location: file_path.to_string(),
                    actual: 0,
                    limit: 0,
                    message: v.message.clone(),
                })
                .collect(),
            warnings_count: warnings,
            errors_count: violations.len() - warnings,
        };

        let artifact =
            Artifact::from_json("constraint_result.json", &artifact_data).map_err(|e| {
                AgentError::Internal {
                    component: "constraint_skill".to_string(),
                    reason: format!("Failed to create artifact: {}", e),
                }
            })?;

        progress.send_update("Constraint check complete.").await?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text(&format!(
                "{} Constraint check: {} violations ({} warnings)",
                if passed { "✅" } else { "⚠️" },
                violations.len(),
                warnings
            ))),
            artifacts: vec![artifact],
        })
    }
}
