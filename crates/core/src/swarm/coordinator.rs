//! # Swarm Coordinator
//!
//! Orchestrates the agent pipeline from user goal to mission prompt.
//! Supports parallel feature processing with configurable concurrency.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Semaphore};

// New: Centralized model types from models module
use crate::models::{LlmProvider, ModelConfig};
use crate::skills::{
    architect_skill::ArchitectOutput, critic_skill::CriticOutput,
    parse_skill::UnknownsParserOutput, researcher_skill::ResearchOutput, ArchitectSkill,
    BuilderSkill, CriticSkill, ParseSkill, ResearcherSkill,
};
use crate::state::{CatalystDb, ProjectState, SpecManager};

use super::events::{SwarmEvent, SwarmEventKind};
use super::pipeline::Pipeline;

/// Configuration for the coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Project mode (speed_run, lab, fortress)
    pub mode: String,
    /// Maximum critic rejections
    pub max_rejections: u32,
    /// Global LLM provider (default: Anthropic)
    #[serde(default)]
    pub global_provider: LlmProvider,
    /// Global model to use for all agents
    pub global_model: Option<String>,
    /// Base URL override for LLM API (for OpenAI-compatible endpoints)
    pub base_url: Option<String>,
    /// Per-agent model overrides (agent_id -> model name)
    pub per_agent_models: HashMap<String, String>,
    /// Per-agent provider overrides (agent_id -> provider)
    #[serde(default)]
    pub per_agent_providers: HashMap<String, LlmProvider>,
    /// Per-agent base URL overrides (agent_id -> base_url, for OpenAI)
    #[serde(default)]
    pub per_agent_base_urls: HashMap<String, String>,
    /// Require human approval for critic rejections
    pub require_critic_approval: bool,
    /// Require human approval for architect decisions
    pub require_architect_approval: bool,
    /// Maximum concurrent features (default: 3)
    pub max_concurrent_features: usize,
    /// Model for WebScraper (cheaper model for HTML cleanup)
    pub scraper_model: Option<String>,
    /// Custom SearXNG instance URL (overrides auto-discovery)
    pub searxng_url: Option<String>,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            mode: "lab".to_string(),
            max_rejections: 3,
            global_provider: LlmProvider::Anthropic,
            global_model: None,
            base_url: None,
            per_agent_models: HashMap::new(),
            per_agent_providers: HashMap::new(),
            per_agent_base_urls: HashMap::new(),
            require_critic_approval: true,
            require_architect_approval: false,
            max_concurrent_features: 3,
            scraper_model: None, // Uses "claude-3-haiku" by default in webscraper
            searxng_url: None,   // Uses auto-discovery by default
        }
    }
}

/// Approval request sent to the UI
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequest {
    pub decision_id: String,
    pub agent_id: String,
    pub summary: String,
}

/// Approval response from the UI
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalResponse {
    pub approved: bool,
    pub feedback: Option<String>,
}

/// Commands sent from API to Coordinator for inbox control
#[derive(Debug)]
pub enum CoordinatorCommand {
    /// Resume coordinator after interaction was resolved
    Resume(String),
    /// Abort the current operation
    Abort,
}

/// Result of running the swarm on unknowns
#[derive(Debug)]
pub struct SwarmResult {
    /// Unknowns that were identified
    pub unknowns: UnknownsParserOutput,
    /// Research results for each unknown
    pub research: Vec<ResearchOutput>,
    /// Architect decisions
    pub decisions: Vec<ArchitectOutput>,
    /// Final critic verdicts
    pub verdicts: Vec<CriticOutput>,
    /// Events that occurred
    pub events: Vec<SwarmEvent>,
    /// Whether the pipeline succeeded
    pub success: bool,
}

/// The swarm coordinator
pub struct Coordinator {
    config: CoordinatorConfig,
    pipeline: Pipeline,
    events: Vec<SwarmEvent>,
    event_tx: Option<mpsc::Sender<SwarmEvent>>,
    approval_tx: Option<mpsc::Sender<(ApprovalRequest, oneshot::Sender<ApprovalResponse>)>>,
    /// Research agent for async research (A2A bridge)
    research_tx: Option<mpsc::Sender<super::a2a_bridge::ResearchMission>>,
    /// Receiver for research progress events
    progress_rx: Option<mpsc::Receiver<super::a2a_bridge::ResearchProgress>>,
    /// Command receiver for inbox signals from API
    command_rx: Option<mpsc::Receiver<CoordinatorCommand>>,
    /// Unified database for all state
    db: Arc<CatalystDb>,
}

impl Coordinator {
    /// Create a new coordinator with a CatalystDb
    pub fn new(config: CoordinatorConfig, db: Arc<CatalystDb>) -> Self {
        let max_rejections = config.max_rejections;
        Self {
            config,
            pipeline: Pipeline {
                max_rejections,
                ..Pipeline::default()
            },
            events: Vec::new(),
            event_tx: None,
            approval_tx: None,
            research_tx: None,
            progress_rx: None,
            command_rx: None,
            db,
        }
    }

    /// Set event channel for streaming events
    pub fn with_event_channel(mut self, tx: mpsc::Sender<SwarmEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set approval channel for human-in-the-loop
    pub fn with_approval_channel(
        mut self,
        tx: mpsc::Sender<(ApprovalRequest, oneshot::Sender<ApprovalResponse>)>,
    ) -> Self {
        self.approval_tx = Some(tx);
        self
    }

    /// Enable async research agent (A2A bridge)
    ///
    /// Spawns a background task that processes research missions.
    /// Research progress events are emitted via the event channel.
    pub fn with_research_agent(mut self) -> Self {
        let (progress_tx, progress_rx) = mpsc::channel(64);
        let config = self.get_model_config("researcher");
        let handle = super::a2a_bridge::spawn_research_agent(config, progress_tx);
        self.research_tx = Some(handle.mission_tx);
        self.progress_rx = Some(progress_rx);
        self
    }

    /// Enable inbox channel for human-in-the-loop interactions
    ///
    /// This allows the coordinator to pause and wait for user responses
    /// via the API. Interactions are persisted to SQLite.
    pub fn with_inbox_channel(mut self, rx: mpsc::Receiver<CoordinatorCommand>) -> Self {
        self.command_rx = Some(rx);
        self
    }

    /// Get model config for a specific agent
    fn get_model_config(&self, agent_id: &str) -> ModelConfig {
        // Get provider: per-agent override -> global -> default
        let provider = self
            .config
            .per_agent_providers
            .get(agent_id)
            .cloned()
            .unwrap_or_else(|| self.config.global_provider.clone());

        // Get model: per-agent override -> global -> default for provider
        let model = self
            .config
            .per_agent_models
            .get(agent_id)
            .or(self.config.global_model.as_ref())
            .cloned()
            .unwrap_or_else(|| match provider {
                LlmProvider::Anthropic => "claude-sonnet-4-20250514".to_string(),
                LlmProvider::OpenAI => "gpt-4o".to_string(),
                LlmProvider::Gemini => "gemini-2.0-flash-exp".to_string(),
                LlmProvider::OpenRouter => "anthropic/claude-3.5-sonnet".to_string(),
                LlmProvider::Grok => "grok-2".to_string(),
                LlmProvider::DeepSeek => "deepseek-chat".to_string(),
            });

        // Get base_url: per-agent override -> global (only for OpenAI)
        let base_url = if provider.supports_base_url() {
            self.config
                .per_agent_base_urls
                .get(agent_id)
                .or(self.config.base_url.as_ref())
                .cloned()
        } else {
            None
        };

        ModelConfig {
            provider,
            model,
            base_url,
        }
    }

    /// Emit an event
    async fn emit(&mut self, event: SwarmEvent) {
        self.events.push(event.clone());
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event).await;
        }
    }

    /// Request human approval
    async fn request_approval(&self, request: ApprovalRequest) -> Option<ApprovalResponse> {
        if let Some(tx) = &self.approval_tx {
            let (resp_tx, resp_rx) = oneshot::channel();
            if tx.send((request, resp_tx)).await.is_ok() {
                return resp_rx.await.ok();
            }
        }
        None
    }

    /// Ask user a question via the Inbox and wait for response
    ///
    /// This blocks the coordinator until the user responds via the API.
    /// The interaction is persisted to SQLite so it survives restarts.
    async fn ask_user(
        &mut self,
        interaction: crate::state::Interaction,
    ) -> Result<crate::state::InteractionResponse> {
        use crate::state::InteractionManager;

        // 1. Open manager and persist to disk
        let mgr = InteractionManager::new(&self.db);
        mgr.save(&interaction)?;

        // 2. Emit event to UI
        self.emit(
            SwarmEvent::new(SwarmEventKind::InteractionRequired, &interaction.from_agent)
                .with_data(serde_json::json!({
                    "interaction_id": interaction.id,
                    "title": interaction.title,
                    "kind": interaction.kind
                })),
        )
        .await;

        // 3. Wait for signal from API
        if let Some(ref mut rx) = self.command_rx {
            loop {
                match rx.recv().await {
                    Some(CoordinatorCommand::Resume(id)) if id == interaction.id => {
                        // Reload from disk to get user's response
                        let mgr = InteractionManager::new(&self.db);
                        let updated = mgr.load(&id)?;

                        // Emit resolved event
                        self.emit(
                            SwarmEvent::new(
                                SwarmEventKind::InteractionResolved,
                                &interaction.from_agent,
                            )
                            .with_data(serde_json::json!({"interaction_id": id})),
                        )
                        .await;

                        return updated.response.ok_or_else(|| {
                            anyhow::anyhow!("Interaction resolved but no response found")
                        });
                    }
                    Some(CoordinatorCommand::Abort) => {
                        anyhow::bail!("User aborted operation");
                    }
                    None => {
                        anyhow::bail!("Coordinator command channel closed");
                    }
                    _ => continue, // Ignore unrelated Resume signals
                }
            }
        } else {
            anyhow::bail!("Inbox channel not configured - use with_inbox_channel()");
        }
    }

    /// Run the swarm on a user goal
    #[tracing::instrument(skip(self), fields(goal_preview = %goal.chars().take(50).collect::<String>()))]
    pub async fn run(&mut self, goal: &str) -> Result<SwarmResult> {
        self.emit(SwarmEvent::new(
            SwarmEventKind::PipelineStarted,
            "coordinator",
        ))
        .await;

        // Load project state
        let mut project_state = ProjectState::load(&self.db).unwrap_or_default();
        project_state.active_agent = Some("coordinator".to_string());
        project_state.phase = "planning".to_string();
        let _ = project_state.save(&self.db);

        // Stage 1: Parse unknowns
        self.emit(SwarmEvent::new(
            SwarmEventKind::AgentStarted,
            "unknowns_parser",
        ))
        .await;

        let model_config = self.get_model_config("unknowns_parser");
        let parse_output = ParseSkill::run(goal, &model_config).await.context(format!(
            "Unknowns Parser failed (provider: {:?}, model: {})",
            model_config.provider, model_config.model
        ))?;

        // Use ParseOutput directly (types are now in parse_skill)
        let unknowns = parse_output;

        self.emit(
            SwarmEvent::new(SwarmEventKind::AgentCompleted, "unknowns_parser")
                .with_data(serde_json::to_value(&unknowns)?),
        )
        .await;

        // Save unknowns to spec fragment
        let unknowns_md = format!(
            "# Unknowns\n\nGenerated by Unknowns Parser\n\n{}\n",
            unknowns
                .ambiguities
                .iter()
                .map(|u| format!(
                    "## [{}] {}\n**Category:** {:?}\n**Priority:** {:?}\n\n{}\n",
                    u.id,
                    u.question,
                    u.category,
                    u.criticality,
                    u.context.as_deref().unwrap_or("")
                ))
                .collect::<Vec<String>>()
                .join("\n")
        );

        // Save unknowns to database (via SpecManager)
        let spec_mgr = SpecManager::new(&self.db);
        if let Err(e) = spec_mgr.write_fragment("unknowns", &unknowns_md) {
            tracing::warn!("Failed to save unknowns: {}", e);
        }

        self.pipeline.advance();

        let mut research_results = Vec::new();
        let mut decisions = Vec::new();
        let mut verdicts = Vec::new();

        // Process each unknown through the pipeline
        for ambiguity in &unknowns.ambiguities {
            // Stage 2: Research (sync or async via A2A bridge)
            let research = if self.research_tx.is_some() {
                // === Async Research via A2A Bridge ===
                self.emit(
                    SwarmEvent::new(SwarmEventKind::ResearchStarted, "researcher")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                // Dispatch mission (clone tx to avoid borrow issues)
                let research_tx = self.research_tx.as_ref().unwrap().clone();
                let (response_tx, response_rx) = oneshot::channel();
                let mission = super::a2a_bridge::ResearchMission {
                    unknown_id: ambiguity.id.clone(),
                    question: ambiguity.question.clone(),
                    context: ambiguity.context.clone().unwrap_or_default(),
                    response_tx,
                };

                if research_tx.send(mission).await.is_err() {
                    anyhow::bail!("Failed to dispatch research mission");
                }

                // Wait for result while draining progress events
                let mut response_rx = response_rx;
                let result = loop {
                    // Try to receive progress events first (non-blocking)
                    if let Some(ref mut progress_rx) = self.progress_rx {
                        match progress_rx.try_recv() {
                            Ok(progress) => {
                                // Convert progress to SwarmEvent
                                let event = match &progress {
                                    super::a2a_bridge::ResearchProgress::Started { unknown_id } => {
                                        SwarmEvent::new(
                                            SwarmEventKind::ResearchStarted,
                                            "researcher",
                                        )
                                        .with_unknown(unknown_id)
                                    }
                                    super::a2a_bridge::ResearchProgress::Status {
                                        unknown_id,
                                        message,
                                    } => SwarmEvent::new(
                                        SwarmEventKind::ResearchProgress,
                                        "researcher",
                                    )
                                    .with_unknown(unknown_id)
                                    .with_data(serde_json::json!({ "message": message })),
                                    super::a2a_bridge::ResearchProgress::Completed {
                                        unknown_id,
                                    } => SwarmEvent::new(
                                        SwarmEventKind::ResearchCompleted,
                                        "researcher",
                                    )
                                    .with_unknown(unknown_id),
                                    super::a2a_bridge::ResearchProgress::Failed {
                                        unknown_id,
                                        error,
                                    } => SwarmEvent::new(SwarmEventKind::AgentFailed, "researcher")
                                        .with_unknown(unknown_id)
                                        .with_data(serde_json::json!({ "error": error })),
                                };
                                self.emit(event).await;
                                continue; // Check for more progress events
                            }
                            Err(mpsc::error::TryRecvError::Empty) => {
                                // No more progress events, check for result
                            }
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                // Progress channel closed, just wait for result
                            }
                        }
                    }

                    // Check if result is ready (with small timeout to allow progress events)
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(50),
                        &mut response_rx,
                    )
                    .await
                    {
                        Ok(Ok(Ok(research))) => break research,
                        Ok(Ok(Err(e))) => {
                            self.emit(
                                SwarmEvent::new(SwarmEventKind::AgentFailed, "researcher")
                                    .with_unknown(&ambiguity.id)
                                    .with_data(serde_json::json!({ "error": e.to_string() })),
                            )
                            .await;
                            anyhow::bail!("Researcher failed: {}", e);
                        }
                        Ok(Err(_)) => anyhow::bail!("Research channel closed unexpectedly"),
                        Err(_) => {
                            // Timeout - loop again to check for progress events
                            continue;
                        }
                    }
                };

                result
            } else {
                // === Sync Research (fallback) ===
                self.emit(
                    SwarmEvent::new(SwarmEventKind::AgentStarted, "researcher")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                let result = ResearcherSkill::run(
                    &ambiguity.id,
                    &ambiguity.question,
                    ambiguity.context.as_deref().unwrap_or(""),
                    &self.get_model_config("researcher"),
                )
                .await
                .context("Researcher failed")?;

                self.emit(
                    SwarmEvent::new(SwarmEventKind::AgentCompleted, "researcher")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                result
            };

            // Stage 3-4: Architect-Critic loop
            let mut attempts = 0;
            let max_attempts = self.config.max_rejections as usize;

            loop {
                attempts += 1;

                // Stage 3: Architect decision
                self.emit(
                    SwarmEvent::new(SwarmEventKind::AgentStarted, "architect")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                let research_json = serde_json::to_string_pretty(&research)?;
                let decision = ArchitectSkill::run(
                    &ambiguity.id,
                    &research_json,
                    "", // Would load spec here
                    &self.config.mode,
                    &self.get_model_config("architect"),
                )
                .await
                .context("Architect failed")?;

                self.emit(
                    SwarmEvent::new(SwarmEventKind::AgentCompleted, "architect")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                // Request approval for architect if configured (uses Inbox)
                if self.config.require_architect_approval {
                    use crate::state::{Interaction, InteractionKind, InteractionStatus};
                    use chrono::Utc;

                    let interaction = Interaction {
                        id: format!("int-arch-{}-{}", ambiguity.id, attempts),
                        thread_id: ambiguity.id.clone(),
                        kind: InteractionKind::Decision,
                        status: InteractionStatus::Pending,
                        from_agent: "architect".to_string(),
                        title: format!("Approve: {}", decision.chosen_option),
                        description: decision.rationale.clone(),
                        options: vec![
                            "Approve".to_string(),
                            "Reject".to_string(),
                            "Modify".to_string(),
                        ],
                        schema: None,
                        created_at: Utc::now(),
                        resolved_at: None,
                        response: None,
                    };

                    // Use inbox if configured, otherwise fall back to old approval channel
                    if self.command_rx.is_some() {
                        match self.ask_user(interaction).await {
                            Ok(response) => {
                                if response.selected_option.as_deref() == Some("Reject") {
                                    continue; // Loop back to re-run architect
                                }
                                // Approve or Modify both continue to critic
                            }
                            Err(e) => {
                                tracing::warn!("Inbox interaction failed: {}, continuing...", e);
                            }
                        }
                    } else {
                        // Fall back to old approval channel
                        let approval = self
                            .request_approval(ApprovalRequest {
                                decision_id: format!("arch-{}-{}", ambiguity.id, attempts),
                                agent_id: "architect".to_string(),
                                summary: format!(
                                    "Approve architect decision for {}?",
                                    ambiguity.id
                                ),
                            })
                            .await;

                        match approval {
                            Some(resp) if !resp.approved => {
                                continue;
                            }
                            _ => {}
                        }
                    }
                }

                // Stage 4: Critic review
                self.emit(
                    SwarmEvent::new(SwarmEventKind::AgentStarted, "critic")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                let decision_json = serde_json::to_string_pretty(&decision)?;
                let verdict = CriticSkill::run(
                    &decision_json,
                    "",
                    &self.config.mode,
                    &self.get_model_config("critic"),
                )
                .await
                .context("Critic failed")?;

                self.emit(
                    SwarmEvent::new(SwarmEventKind::AgentCompleted, "critic")
                        .with_unknown(&ambiguity.id),
                )
                .await;

                if verdict.verdict == "approved" {
                    decisions.push(decision);
                    verdicts.push(verdict);
                    break;
                } else if attempts >= max_attempts {
                    // Max rejections reached
                    self.emit(
                        SwarmEvent::new(SwarmEventKind::CriticRejected, "critic")
                            .with_unknown(&ambiguity.id)
                            .with_data(
                                serde_json::json!({"attempts": attempts, "max": max_attempts}),
                            ),
                    )
                    .await;

                    // Request human approval if configured
                    if self.config.require_critic_approval {
                        let approval = self
                            .request_approval(ApprovalRequest {
                                decision_id: format!("crit-{}-{}", ambiguity.id, attempts),
                                agent_id: "critic".to_string(),
                                summary: format!(
                                    "Critic rejected {} times. Override and approve?",
                                    attempts
                                ),
                            })
                            .await;

                        match approval {
                            Some(resp) if resp.approved => {
                                // User overrode, accept decision
                                decisions.push(decision);
                            }
                            _ => {}
                        }
                    }

                    verdicts.push(verdict);
                    break;
                } else {
                    // Loop back to architect
                    self.emit(
                        SwarmEvent::new(SwarmEventKind::CriticRejected, "critic")
                            .with_unknown(&ambiguity.id),
                    )
                    .await;
                }
            }

            research_results.push(research);
        }

        let success = verdicts.iter().all(|v| v.verdict == "approved");

        self.emit(SwarmEvent::new(
            if success {
                SwarmEventKind::PipelineCompleted
            } else {
                SwarmEventKind::PipelineFailed
            },
            "coordinator",
        ))
        .await;

        // Update project state
        project_state.active_agent = None;
        project_state.phase = if success { "execution_ready" } else { "failed" }.to_string();
        let _ = project_state.save(&self.db);

        Ok(SwarmResult {
            unknowns,
            research: research_results,
            decisions,
            verdicts,
            events: self.events.clone(),
            success,
        })
    }

    /// Execute the Speed Demon drafting phase
    ///
    /// Scatter-Gather pattern: fires off parallel LLM calls for each file,
    /// then bulk writes all results to the worktree.
    ///
    /// # Arguments
    /// * `missions` - List of drafting missions from Taskmaster
    /// * `worktree_path` - Target directory for bulk write
    ///
    /// # Events Emitted
    /// * `DraftingStarted` - At phase start with total count
    /// * `DraftingProgress` - After each file completes
    /// * `DraftingCompleted` - After bulk write
    pub async fn execute_drafting_phase(
        &mut self,
        missions: Vec<crate::skills::drafting_skill::DraftingMission>,
        worktree_path: &std::path::Path,
    ) -> Result<Vec<crate::skills::drafting_skill::DraftingOutput>> {
        use crate::skills::DraftingSkill;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::task::JoinSet;

        let total = missions.len();
        if total == 0 {
            return Ok(Vec::new());
        }

        // Emit start event
        self.emit(
            SwarmEvent::new(SwarmEventKind::DraftingStarted, "drafter")
                .with_data(serde_json::json!({ "total": total })),
        )
        .await;

        let config = Arc::new(self.get_model_config("drafter"));
        let completed = Arc::new(AtomicUsize::new(0));
        let event_tx = self.event_tx.clone();

        let mut join_set = JoinSet::new();

        // SCATTER: Fire off parallel drafts
        for mission in missions {
            let cfg = config.clone();
            let completed = completed.clone();
            let tx = event_tx.clone();
            let file_path = mission.file_path.clone();

            join_set.spawn(async move {
                let result = DraftingSkill::draft(&mission, &cfg).await;

                // Update progress counter and emit event
                let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(tx) = tx {
                    let _ = tx
                        .send(
                            SwarmEvent::new(SwarmEventKind::DraftingProgress, "drafter").with_data(
                                serde_json::json!({
                                    "completed": done,
                                    "total": total,
                                    "file_path": file_path
                                }),
                            ),
                        )
                        .await;
                }

                result
            });
        }

        // GATHER: Collect all outputs
        let mut outputs = Vec::with_capacity(total);
        let mut errors = Vec::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(output)) => outputs.push(output),
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(e) => errors.push(format!("Task panicked: {}", e)),
            }
        }

        // Report errors but continue with successful outputs
        if !errors.is_empty() {
            tracing::warn!(
                "Drafting had {} errors out of {}: {:?}",
                errors.len(),
                total,
                errors
            );
        }

        // BULK WRITE: Write all files to worktree
        for output in &outputs {
            let path = worktree_path.join(&output.file_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create directory for {}", output.file_path)
                })?;
            }
            std::fs::write(&path, &output.source_code)
                .with_context(|| format!("Failed to write {}", output.file_path))?;
        }

        // Emit completion event
        self.emit(
            SwarmEvent::new(SwarmEventKind::DraftingCompleted, "drafter").with_data(
                serde_json::json!({
                    "files_written": outputs.len(),
                    "errors": errors.len()
                }),
            ),
        )
        .await;

        Ok(outputs)
    }

    /// Run multiple features in parallel with concurrency control
    ///
    /// Uses a Semaphore to limit concurrent feature processing.
    pub async fn run_features_parallel(
        &self,
        feature_ids: Vec<String>,
    ) -> Result<Vec<FeatureResult>> {
        use crate::state::{FeatureManager, PipelineStage};
        use crate::tools::git;

        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_features));
        let builder_config = Arc::new(self.get_model_config("builder"));
        let event_tx = self.event_tx.clone();
        let db = Arc::clone(&self.db);

        let mut handles = Vec::new();

        for feature_id in feature_ids {
            let permit = semaphore.clone().acquire_owned().await?;
            let builder_config = builder_config.clone();
            let event_tx = event_tx.clone();
            let feature_id = feature_id.clone();
            let db = Arc::clone(&db);

            let handle = tokio::spawn(async move {
                let _permit = permit; // Hold permit until task completes
                let fm = FeatureManager::new(&db);

                // Load feature
                let feature = match fm.load(&feature_id) {
                    Ok(f) => f,
                    Err(_) => {
                        return FeatureResult {
                            feature_id,
                            success: false,
                            error: Some("Failed to load feature".to_string()),
                        };
                    }
                };

                // Update stage to Building
                let _ = fm.update_stage(&feature_id, PipelineStage::Building);

                // Create worktree for this feature
                let project_root = std::env::current_dir().unwrap_or_default();
                let worktree_path = match git::create_worktree(&project_root, &feature_id) {
                    Ok(path) => path,
                    Err(e) => {
                        let _ = fm.set_failed(&feature_id, &e.to_string());
                        return FeatureResult {
                            feature_id,
                            success: false,
                            error: Some(e.to_string()),
                        };
                    }
                };
                let _ = fm.set_worktree(&feature_id, worktree_path.clone());

                // Emit event if channel available
                if let Some(tx) = &event_tx {
                    let _ = tx
                        .send(SwarmEvent::new(SwarmEventKind::AgentStarted, "builder"))
                        .await;
                }

                // Run Builder agent in worktree
                let mission = feature
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Implement feature: {}", feature.title));

                let builder_result =
                    BuilderSkill::run(&mission, &worktree_path, builder_config.as_ref()).await;

                match builder_result {
                    Ok(output) if output.success => {
                        let _ = fm.update_stage(&feature_id, PipelineStage::Testing);
                    }
                    Ok(output) => {
                        // Build failed after iterations
                        let _ = fm.set_failed(&feature_id, &output.summary);
                        return FeatureResult {
                            feature_id,
                            success: false,
                            error: Some(output.summary),
                        };
                    }
                    Err(e) => {
                        let _ = fm.set_failed(&feature_id, &e.to_string());
                        return FeatureResult {
                            feature_id,
                            success: false,
                            error: Some(e.to_string()),
                        };
                    }
                }

                // TODO: Run tests (RedTeam agent)

                // Merge back
                let merge_result = git::merge_worktree(&project_root, &feature_id);

                match merge_result {
                    Ok(git::MergeResult::Success) => {
                        let _ = fm.update_stage(&feature_id, PipelineStage::Complete);
                        let _ = git::delete_worktree(&project_root, &feature_id);

                        FeatureResult {
                            feature_id,
                            success: true,
                            error: None,
                        }
                    }
                    Ok(git::MergeResult::Conflicts(files)) => {
                        let _ = fm.update_stage(&feature_id, PipelineStage::Merging);
                        FeatureResult {
                            feature_id,
                            success: false,
                            error: Some(format!("Merge conflicts in: {:?}", files)),
                        }
                    }
                    Err(e) => {
                        let _ = fm.set_failed(&feature_id, &e.to_string());
                        FeatureResult {
                            feature_id,
                            success: false,
                            error: Some(e.to_string()),
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Await all feature tasks
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(FeatureResult {
                    feature_id: "unknown".to_string(),
                    success: false,
                    error: Some(e.to_string()),
                }),
            }
        }

        Ok(results)
    }
}

/// Result of processing a single feature
#[derive(Debug, Clone, Serialize)]
pub struct FeatureResult {
    pub feature_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_config_default() {
        let config = CoordinatorConfig::default();
        assert_eq!(config.mode, "lab");
        assert_eq!(config.max_rejections, 3);
    }
}
