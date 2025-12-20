//! Catalyst Server
//!
//! Axum server that embeds and serves the React frontend with API routes.
//! Fully wired to the real Coordinator from crates/core with approval support.

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, Response, StatusCode, Uri},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{any, delete, get, post},
    Router,
};
use catalyst_core::memory::{CatalystMemory, MemoryConfig};
use catalyst_core::models::LlmProvider;
use catalyst_core::state::CatalystDb;
use catalyst_core::swarm::{
    ApprovalRequest, ApprovalResponse, Coordinator, CoordinatorConfig, SwarmEvent,
};
use clap::{Parser, Subcommand};
use futures::{
    stream::{self, Stream},
    SinkExt, StreamExt,
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc, oneshot, RwLock},
};
use utoipa::{OpenApi, ToSchema};

/// Embedded frontend assets
#[derive(RustEmbed)]
#[folder = "../../apps/frontend/dist"]
struct Assets;

/// Application state
struct AppState {
    swarm_status: RwLock<SwarmStatus>,
    event_tx: broadcast::Sender<SwarmEvent>,
    memory: CatalystMemory,
    /// Unified database for all state
    db: Arc<CatalystDb>,
    // Pending approval requests: decision_id -> oneshot sender
    pending_approvals: RwLock<HashMap<String, oneshot::Sender<ApprovalResponse>>>,
    // Channel to send commands to the coordinator (for inbox) - updated per swarm run
    coordinator_tx: RwLock<Option<mpsc::Sender<catalyst_core::swarm::CoordinatorCommand>>>,
}

#[derive(Default, Clone, Serialize, ToSchema)]
struct SwarmStatus {
    status: String,
    active_agent: Option<String>,
    pipeline_stage: u8,
}

type SharedState = Arc<AppState>;

// === API Types ===

#[derive(Deserialize, ToSchema)]
struct StartSwarmRequest {
    goal: String,
    settings: Option<ApiSettings>,
}

#[derive(Deserialize, ToSchema)]
struct ApiSettings {
    global_provider: Option<String>,
    global_model: Option<String>,
    base_url: Option<String>,
    #[serde(skip)]
    #[schema(ignore)]
    per_agent_providers: Option<HashMap<String, String>>,
    #[serde(skip)]
    #[schema(ignore)]
    per_agent_models: Option<HashMap<String, String>>,
    #[serde(skip)]
    #[schema(ignore)]
    per_agent_base_urls: Option<HashMap<String, String>>,
    mode: Option<String>,
    require_critic_approval: Option<bool>,
    require_architect_approval: Option<bool>,
    max_concurrent_features: Option<usize>,
    max_rejections: Option<u32>,
    scraper_model: Option<String>,
    searxng_url: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct ApiResponse {
    success: bool,
    message: String,
}

#[derive(Deserialize, ToSchema)]
struct ApprovalApiRequest {
    decision_id: String,
    approved: bool,
    feedback: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct ApiKeysRequest {
    anthropic: Option<String>,
    openai: Option<String>,
    gemini: Option<String>,
    openrouter: Option<String>,
    grok: Option<String>,
    deepseek: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct MemorySearchRequest {
    query: String,
}

#[derive(Serialize, ToSchema)]
struct MemorySearchResponse {
    results: Vec<MemoryResult>,
}

#[derive(Serialize, ToSchema)]
struct MemoryResult {
    id: String,
    text: String,
    source: String,
    score: f32,
}

// === Braindump API Types ===

#[derive(Deserialize, ToSchema)]
struct CreateIdeaRequest {
    content: String,
}

#[derive(Serialize, ToSchema)]
struct IdeaResponse {
    id: String,
    content: String,
    created_at: String,
}

#[derive(Serialize, ToSchema)]
struct ContextFileResponse {
    path: String,
    size: u64,
    extension: Option<String>,
    ingested_at: String,
}

#[derive(Serialize, ToSchema)]
struct BraindumpListResponse {
    ideas: Vec<IdeaResponse>,
    files: Vec<ContextFileResponse>,
}

// === Reactor API Types ===

#[derive(Serialize, ToSchema)]
struct FeatureResponse {
    id: String,
    title: String,
    stage: String,
    description: Option<String>,
    created_at: String,
}

#[derive(Deserialize, ToSchema)]
struct IgniteRequest {
    idea_id: String,
}

#[derive(Serialize, ToSchema)]
struct IgniteResponse {
    success: bool,
    feature_id: Option<String>,
    error: Option<String>,
}

// === Snapshot/Rollback API Types ===

#[derive(Serialize, ToSchema)]
struct SnapshotResponse {
    id: String,
    stage: String,
    timestamp: String,
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct RollbackRequest {
    snapshot_id: String,
}

#[derive(Serialize, ToSchema)]
struct RollbackResponse {
    success: bool,
    snapshot_id: String,
    stage: String,
    message: String,
}

// === Inbox API Types ===

#[derive(Serialize, ToSchema)]
struct InboxItem {
    id: String,
    kind: String,
    from_agent: String,
    title: String,
    description: String,
    options: Vec<String>,
    created_at: String,
}

#[derive(Serialize, ToSchema)]
struct InboxListResponse {
    items: Vec<InboxItem>,
}

#[derive(Deserialize, ToSchema)]
struct InboxReplyRequest {
    selected_option: Option<String>,
    text_input: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct InboxReplyResponse {
    success: bool,
    message: String,
}

// === Swarm Control Types ===

#[derive(Deserialize, ToSchema)]
struct StopSwarmRequest {
    /// Optional reason for stopping
    reason: Option<String>,
}

// === Codebase Profile Types ===

#[derive(Serialize, ToSchema)]
struct ProfileResponse {
    exists: bool,
    profile: Option<CodebaseProfileResponse>,
}

#[derive(Serialize, ToSchema)]
struct CodebaseProfileResponse {
    project_type: String,
    total_files: u32,
    total_loc: u32,
    frameworks: Vec<String>,
    style_patterns: StylePatternsResponse,
    scanned_at: String,
}

#[derive(Serialize, ToSchema)]
struct StylePatternsResponse {
    naming_convention: Option<String>,
    error_handling: Option<String>,
    async_runtime: Option<String>,
    test_framework: Option<String>,
}

// === Prompt Template Types ===

#[derive(Serialize, ToSchema)]
struct PromptListItem {
    slug: String,
    version: i32,
}

#[derive(Serialize, ToSchema)]
struct PromptListResponse {
    prompts: Vec<PromptListItem>,
}

#[derive(Serialize, ToSchema)]
struct PromptResponse {
    slug: String,
    content: String,
    version: i32,
}

#[derive(Deserialize, ToSchema)]
struct UpdatePromptRequest {
    content: String,
}

#[derive(Serialize, ToSchema)]
struct UpdatePromptResponse {
    success: bool,
    slug: String,
    new_version: i32,
}

// === Project Document Types ===

#[derive(Serialize, ToSchema)]
struct DocumentListResponse {
    documents: Vec<String>,
}

#[derive(Serialize, ToSchema)]
struct DocumentResponse {
    slug: String,
    title: String,
    content: String,
}

#[derive(Deserialize, ToSchema)]
struct UpdateDocumentRequest {
    title: String,
    content: String,
}

#[derive(Parser, Clone)]
#[command(author, version, about = "Catalyst - Autonomous Coding Agent Swarm")]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand, Clone)]
enum CliCommand {
    /// Start the Catalyst server (default)
    Serve {
        /// Run in development mode (hot-reloading frontend)
        #[arg(long)]
        dev: bool,
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    /// Initialize a new Catalyst project in the current directory
    Init {
        /// Project name
        #[arg(short, long)]
        name: Option<String>,
        /// Project description  
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Run the swarm on a goal (CLI mode, no server)
    Run {
        /// The goal to accomplish
        goal: String,
    },
}

// === Config API Types ===

/// Persisted configuration (subset of CoordinatorConfig exposed to frontend)
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
struct PersistedConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    global_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    global_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    require_critic_approval: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    require_architect_approval: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_concurrent_features: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_rejections: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scraper_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    searxng_url: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    per_agent_providers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    per_agent_models: HashMap<String, String>,
}

impl PersistedConfig {
    async fn load() -> Self {
        let path = std::path::PathBuf::from(".catalyst/config.json");
        if path.exists() {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    async fn save(&self) -> Result<(), std::io::Error> {
        let path = std::path::PathBuf::from(".catalyst/config.json");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        tokio::fs::write(&path, content).await
    }

    fn merge(&mut self, other: PersistedConfig) {
        if other.global_provider.is_some() {
            self.global_provider = other.global_provider;
        }
        if other.global_model.is_some() {
            self.global_model = other.global_model;
        }
        if other.base_url.is_some() {
            self.base_url = other.base_url;
        }
        if other.mode.is_some() {
            self.mode = other.mode;
        }
        if other.require_critic_approval.is_some() {
            self.require_critic_approval = other.require_critic_approval;
        }
        if other.require_architect_approval.is_some() {
            self.require_architect_approval = other.require_architect_approval;
        }
        if other.max_concurrent_features.is_some() {
            self.max_concurrent_features = other.max_concurrent_features;
        }
        if other.max_rejections.is_some() {
            self.max_rejections = other.max_rejections;
        }
        if other.scraper_model.is_some() {
            self.scraper_model = other.scraper_model;
        }
        if other.searxng_url.is_some() {
            self.searxng_url = other.searxng_url;
        }
        for (k, v) in other.per_agent_providers {
            self.per_agent_providers.insert(k, v);
        }
        for (k, v) in other.per_agent_models {
            self.per_agent_models.insert(k, v);
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigResponse {
    config: PersistedConfig,
    defaults: ConfigDefaults,
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigDefaults {
    global_provider: &'static str,
    mode: &'static str,
    max_concurrent_features: usize,
    max_rejections: u32,
    require_critic_approval: bool,
    require_architect_approval: bool,
}

impl Default for ConfigDefaults {
    fn default() -> Self {
        Self {
            global_provider: "anthropic",
            mode: "lab",
            max_concurrent_features: 3,
            max_rejections: 3,
            require_critic_approval: true,
            require_architect_approval: false,
        }
    }
}

// === Provider API Types ===

#[derive(Debug, Serialize, ToSchema)]
struct ProviderInfo {
    id: String,
    name: String,
    default_model: String,
    supports_base_url: bool,
    env_var: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct ProvidersResponse {
    providers: Vec<ProviderInfo>,
}

fn get_provider_info() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            default_model: "claude-sonnet-4-20250514".to_string(),
            supports_base_url: false,
            env_var: "ANTHROPIC_API_KEY".to_string(),
        },
        ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            default_model: "gpt-4o".to_string(),
            supports_base_url: true,
            env_var: "OPENAI_API_KEY".to_string(),
        },
        ProviderInfo {
            id: "gemini".to_string(),
            name: "Gemini".to_string(),
            default_model: "gemini-2.0-flash-exp".to_string(),
            supports_base_url: false,
            env_var: "GEMINI_API_KEY".to_string(),
        },
        ProviderInfo {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            default_model: "anthropic/claude-3.5-sonnet".to_string(),
            supports_base_url: false,
            env_var: "OPENROUTER_API_KEY".to_string(),
        },
        ProviderInfo {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            default_model: "grok-2".to_string(),
            supports_base_url: false,
            env_var: "XAI_API_KEY".to_string(),
        },
        ProviderInfo {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            default_model: "deepseek-chat".to_string(),
            supports_base_url: false,
            env_var: "DEEPSEEK_API_KEY".to_string(),
        },
    ]
}

// === OpenAPI Definition ===

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Catalyst API",
        version = "1.0.0",
        description = "API for the Catalyst Autonomous Coding Agent Swarm"
    ),
    paths(
        get_status,
        start_swarm,
        stop_swarm,
        handle_approval,
        get_config,
        update_config,
        get_providers,
        search_memory,
        list_braindump,
        create_idea,
        list_features,
        delete_feature,
        ignite_feature,
        list_snapshots,
        delete_snapshot,
        rollback_to_snapshot,
        list_inbox,
        reply_to_inbox,
        list_inbox_history,
        get_project_status,
        get_project_profile,
        init_project,
        list_prompts,
        get_prompt,
        update_prompt,
        list_documents,
        get_document,
        update_document,
        save_api_keys
    ),
    components(
        schemas(
            SwarmStatus,
            ApiResponse,
            StartSwarmRequest,
            StopSwarmRequest,
            ApiSettings,
            ApprovalApiRequest,
            ApiKeysRequest,
            ConfigResponse,
            ConfigDefaults,
            PersistedConfig,
            ProvidersResponse,
            ProviderInfo,
            MemorySearchRequest,
            MemorySearchResponse,
            MemoryResult,
            CreateIdeaRequest,
            BraindumpListResponse,
            IdeaResponse,
            ContextFileResponse,
            FeatureResponse,
            IgniteRequest,
            IgniteResponse,
            SnapshotResponse,
            RollbackRequest,
            RollbackResponse,
            InboxItem,
            InboxListResponse,
            InboxReplyRequest,
            InboxReplyResponse,
            ProjectStatusResponse,
            InitProjectRequest,
            InitProjectResponse,
            ProfileResponse,
            CodebaseProfileResponse,
            StylePatternsResponse,
            PromptListResponse,
            PromptListItem,
            PromptResponse,
            UpdatePromptRequest,
            UpdatePromptResponse,
            DocumentListResponse,
            DocumentResponse,
            UpdateDocumentRequest
        )
    ),
    tags(
        (name = "swarm", description = "Swarm management endpoints"),
        (name = "config", description = "Configuration management"),
        (name = "providers", description = "LLM provider discovery"),
        (name = "memory", description = "Memory search"),
        (name = "braindump", description = "Ideas and context management"),
        (name = "reactor", description = "Feature pipeline management"),
        (name = "inbox", description = "Human-in-the-loop interactions"),
        (name = "project", description = "Project initialization"),
        (name = "prompts", description = "Prompt template management"),
        (name = "documents", description = "Project document management")
    )
)]
struct ApiDoc;

// === API Handlers ===

/// Get swarm status
#[utoipa::path(
    get,
    path = "/api/v1/swarm/status",
    tag = "swarm",
    responses(
        (status = 200, description = "Current swarm status", body = SwarmStatus)
    )
)]
async fn get_status(State(state): State<SharedState>) -> Json<SwarmStatus> {
    let status = state.swarm_status.read().await;
    Json(status.clone())
}

/// Start the swarm with a goal
#[utoipa::path(
    post,
    path = "/api/v1/swarm/start",
    tag = "swarm",
    request_body = StartSwarmRequest,
    responses(
        (status = 200, description = "Swarm started", body = ApiResponse)
    )
)]
async fn start_swarm(
    State(state): State<SharedState>,
    Json(req): Json<StartSwarmRequest>,
) -> Json<ApiResponse> {
    {
        let mut status = state.swarm_status.write().await;
        status.status = "running".to_string();
        status.active_agent = Some("unknowns_parser".to_string());
        status.pipeline_stage = 1;
    }

    println!("🚀 Starting swarm with goal: {}", req.goal);

    // Build config from settings
    let mut config = CoordinatorConfig::default();
    if let Some(settings) = &req.settings {
        // Map global provider from string to enum
        if let Some(ref p) = settings.global_provider {
            config.global_provider = match p.as_str() {
                "anthropic" => LlmProvider::Anthropic,
                "openai" => LlmProvider::OpenAI,
                "gemini" => LlmProvider::Gemini,
                "openrouter" => LlmProvider::OpenRouter,
                "grok" => LlmProvider::Grok,
                "deepseek" => LlmProvider::DeepSeek,
                _ => LlmProvider::Anthropic, // fallback
            };
        }
        if let Some(ref m) = settings.global_model {
            config.global_model = Some(m.clone());
        }
        if let Some(ref url) = settings.base_url {
            config.base_url = Some(url.clone());
        }
        // Map per-agent providers from strings to enums
        if let Some(ref providers) = settings.per_agent_providers {
            for (agent, provider_str) in providers {
                let provider = match provider_str.as_str() {
                    "anthropic" => LlmProvider::Anthropic,
                    "openai" => LlmProvider::OpenAI,
                    "gemini" => LlmProvider::Gemini,
                    "openrouter" => LlmProvider::OpenRouter,
                    "grok" => LlmProvider::Grok,
                    "deepseek" => LlmProvider::DeepSeek,
                    _ => continue, // skip invalid
                };
                config.per_agent_providers.insert(agent.clone(), provider);
            }
        }
        if let Some(ref models) = settings.per_agent_models {
            config.per_agent_models = models.clone();
        }
        if let Some(ref urls) = settings.per_agent_base_urls {
            config.per_agent_base_urls = urls.clone();
        }
        if let Some(ref mode) = settings.mode {
            config.mode = mode.clone();
        }
        if let Some(crit) = settings.require_critic_approval {
            config.require_critic_approval = crit;
        }
        if let Some(arch) = settings.require_architect_approval {
            config.require_architect_approval = arch;
        }
        // New fields
        if let Some(max_conc) = settings.max_concurrent_features {
            config.max_concurrent_features = max_conc;
        }
        if let Some(max_rej) = settings.max_rejections {
            config.max_rejections = max_rej;
        }
        if let Some(ref scraper) = settings.scraper_model {
            config.scraper_model = Some(scraper.clone());
        }
        if let Some(ref searx) = settings.searxng_url {
            // Also set the environment variable for search_tools to pick up
            std::env::set_var("SEARXNG_URL", searx);
            config.searxng_url = Some(searx.clone());
        }
    }

    // Create channels
    let (event_mpsc_tx, mut event_mpsc_rx) = mpsc::channel::<SwarmEvent>(100);
    let (approval_tx, mut approval_rx) =
        mpsc::channel::<(ApprovalRequest, oneshot::Sender<ApprovalResponse>)>(10);

    // Create inbox command channel for human-in-the-loop
    let (inbox_tx, inbox_rx) = mpsc::channel::<catalyst_core::swarm::CoordinatorCommand>(10);

    // Store the inbox sender in state so reply_to_inbox can access it
    *state.coordinator_tx.write().await = Some(inbox_tx);

    let broadcast_tx = state.event_tx.clone();
    let state_clone = state.clone();
    let goal = req.goal.clone();

    // Bridge events to broadcast
    tokio::spawn(async move {
        while let Some(event) = event_mpsc_rx.recv().await {
            let _ = broadcast_tx.send(event);
        }
    });

    // Handle approval requests
    let state_approval = state.clone();
    tokio::spawn(async move {
        while let Some((request, responder)) = approval_rx.recv().await {
            println!("⚠️ Approval needed: {}", request.summary);
            state_approval
                .pending_approvals
                .write()
                .await
                .insert(request.decision_id.clone(), responder);
        }
    });

    // Run the coordinator
    let db = Arc::clone(&state.db);
    tokio::spawn(async move {
        let mut coordinator = Coordinator::new(config, db)
            .with_event_channel(event_mpsc_tx)
            .with_approval_channel(approval_tx)
            .with_inbox_channel(inbox_rx)
            .with_research_agent();

        match coordinator.run(&goal).await {
            Ok(result) => {
                println!("✅ Swarm completed: success={}", result.success);
                let mut status = state_clone.swarm_status.write().await;
                status.status = if result.success { "complete" } else { "failed" }.to_string();
                status.active_agent = None;
            }
            Err(e) => {
                eprintln!("❌ Swarm failed: {}", e);
                let mut status = state_clone.swarm_status.write().await;
                status.status = "error".to_string();
                status.active_agent = None;
            }
        }
    });

    Json(ApiResponse {
        success: true,
        message: format!("Swarm started with goal: {}", req.goal),
    })
}

/// Stop/abort the running swarm
#[utoipa::path(
    post,
    path = "/api/v1/swarm/stop",
    tag = "swarm",
    request_body = StopSwarmRequest,
    responses(
        (status = 200, description = "Swarm stopped", body = ApiResponse)
    )
)]
async fn stop_swarm(
    State(state): State<SharedState>,
    Json(req): Json<StopSwarmRequest>,
) -> Json<ApiResponse> {
    // Send abort command if coordinator is running
    if let Some(tx) = state.coordinator_tx.read().await.as_ref() {
        let _ = tx
            .send(catalyst_core::swarm::CoordinatorCommand::Abort)
            .await;
    }
    // Update status
    let mut status = state.swarm_status.write().await;
    status.status = "stopped".to_string();
    status.active_agent = None;

    Json(ApiResponse {
        success: true,
        message: req.reason.unwrap_or_else(|| "Swarm stopped".to_string()),
    })
}

/// Handle approval/rejection of pending decisions
#[utoipa::path(
    post,
    path = "/api/v1/swarm/approve",
    tag = "swarm",
    request_body = ApprovalApiRequest,
    responses(
        (status = 200, description = "Approval processed", body = ApiResponse)
    )
)]
async fn handle_approval(
    State(state): State<SharedState>,
    Json(req): Json<ApprovalApiRequest>,
) -> Json<ApiResponse> {
    let responder = state
        .pending_approvals
        .write()
        .await
        .remove(&req.decision_id);

    if let Some(tx) = responder {
        let response = ApprovalResponse {
            approved: req.approved,
            feedback: req.feedback.clone(),
        };
        let _ = tx.send(response);

        println!(
            "{} decision: {}",
            if req.approved {
                "✅ Approved"
            } else {
                "❌ Rejected"
            },
            req.decision_id
        );

        Json(ApiResponse {
            success: true,
            message: if req.approved { "Approved" } else { "Rejected" }.to_string(),
        })
    } else {
        Json(ApiResponse {
            success: false,
            message: "No pending approval with that ID".to_string(),
        })
    }
}

/// Search memory
#[utoipa::path(
    post,
    path = "/api/v1/memory/search",
    tag = "memory",
    request_body = MemorySearchRequest,
    responses(
        (status = 200, description = "Search results", body = MemorySearchResponse)
    )
)]
async fn search_memory(
    State(state): State<SharedState>,
    Json(req): Json<MemorySearchRequest>,
) -> Json<MemorySearchResponse> {
    match state.memory.search(&req.query).await {
        Ok(texts) => {
            let results: Vec<MemoryResult> = texts
                .into_iter()
                .enumerate()
                .map(|(i, text)| MemoryResult {
                    id: format!("mem-{}", i),
                    text: text.chars().take(200).collect(), // Truncate for display
                    source: "database".to_string(),
                    score: 1.0 - (i as f32 * 0.1), // Approximate score
                })
                .collect();
            Json(MemorySearchResponse { results })
        }
        Err(e) => {
            eprintln!("Memory search failed: {}", e);
            Json(MemorySearchResponse { results: vec![] })
        }
    }
}

/// SSE endpoint for real-time events with heartbeat
async fn events(
    State(state): State<SharedState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();

    // Use timeout-based stream with heartbeat every 15 seconds
    let stream = stream::unfold(
        (rx, tokio::time::Instant::now()),
        |(mut rx, _last_event)| async move {
            let timeout = tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv()).await;

            match timeout {
                Ok(Ok(event)) => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    Some((
                        Ok(Event::default().data(json)),
                        (rx, tokio::time::Instant::now()),
                    ))
                }
                Ok(Err(_)) => None, // Channel closed
                Err(_) => {
                    // Timeout - send heartbeat comment
                    Some((
                        Ok(Event::default().comment("heartbeat")),
                        (rx, tokio::time::Instant::now()),
                    ))
                }
            }
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Save API keys to .catalyst/.env
#[utoipa::path(
    post,
    path = "/api/v1/settings/api-keys",
    tag = "config",
    request_body = ApiKeysRequest,
    responses(
        (status = 200, description = "API keys saved", body = ApiResponse)
    )
)]
async fn save_api_keys(Json(req): Json<ApiKeysRequest>) -> Json<ApiResponse> {
    use std::fs;
    use std::path::Path;

    let catalyst_dir = Path::new(".catalyst");

    // Create .catalyst directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(catalyst_dir) {
        return Json(ApiResponse {
            success: false,
            message: format!("Failed to create .catalyst directory: {}", e),
        });
    }

    // Create .gitignore to protect API keys
    let gitignore_path = catalyst_dir.join(".gitignore");
    if !gitignore_path.exists() {
        let _ = fs::write(&gitignore_path, "# Never commit API keys\n.env\n*.env\n");
    }

    // Build .env content
    let mut env_content =
        String::from("# Catalyst API Keys - DO NOT COMMIT\n# Generated by Catalyst UI\n\n");

    if let Some(key) = &req.anthropic {
        if !key.is_empty() {
            env_content.push_str(&format!("ANTHROPIC_API_KEY={}\n", key));
        }
    }
    if let Some(key) = &req.openai {
        if !key.is_empty() {
            env_content.push_str(&format!("OPENAI_API_KEY={}\n", key));
        }
    }
    if let Some(key) = &req.gemini {
        if !key.is_empty() {
            env_content.push_str(&format!("GOOGLE_API_KEY={}\n", key));
        }
    }
    if let Some(key) = &req.openrouter {
        if !key.is_empty() {
            env_content.push_str(&format!("OPENROUTER_API_KEY={}\n", key));
        }
    }
    if let Some(key) = &req.grok {
        if !key.is_empty() {
            env_content.push_str(&format!("XAI_API_KEY={}\n", key));
        }
    }
    if let Some(key) = &req.deepseek {
        if !key.is_empty() {
            env_content.push_str(&format!("DEEPSEEK_API_KEY={}\n", key));
        }
    }

    // Write .env file
    let env_path = catalyst_dir.join(".env");
    match fs::write(&env_path, env_content) {
        Ok(_) => {
            // Load the .env file for current process immediately
            let _ = dotenvy::from_path(&env_path);
            Json(ApiResponse {
                success: true,
                message: "API keys saved and loaded".to_string(),
            })
        }
        Err(e) => Json(ApiResponse {
            success: false,
            message: format!("Failed to write .env file: {}", e),
        }),
    }
}

// === Braindump API Handlers ===

/// List all ideas and context files
#[utoipa::path(
    get,
    path = "/api/v1/braindump",
    tag = "braindump",
    responses(
        (status = 200, description = "List of ideas and context files", body = BraindumpListResponse)
    )
)]
async fn list_braindump(State(state): State<SharedState>) -> Json<BraindumpListResponse> {
    use catalyst_core::state::ContextManager;

    let cm = ContextManager::new(&state.db);
    let ideas = match cm.list_ideas() {
        Ok(ideas) => ideas
            .into_iter()
            .map(|i| IdeaResponse {
                id: i.id,
                content: i.content,
                created_at: i.created_at.to_rfc3339(),
            })
            .collect(),
        Err(_) => vec![],
    };

    let files = match cm.list_files() {
        Ok(files) => files
            .into_iter()
            .map(|f| ContextFileResponse {
                path: f.path,
                size: f.size,
                extension: f.extension,
                ingested_at: f.ingested_at.to_rfc3339(),
            })
            .collect(),
        Err(_) => vec![],
    };

    Json(BraindumpListResponse { ideas, files })
}

/// Create a new idea
#[utoipa::path(
    post,
    path = "/api/v1/braindump/ideas",
    tag = "braindump",
    request_body = CreateIdeaRequest,
    responses(
        (status = 200, description = "Idea created", body = ApiResponse)
    )
)]
async fn create_idea(
    State(state): State<SharedState>,
    Json(req): Json<CreateIdeaRequest>,
) -> Json<ApiResponse> {
    use catalyst_core::state::ContextManager;

    let cm = ContextManager::new(&state.db);
    match cm.create_idea(&req.content) {
        Ok(idea) => Json(ApiResponse {
            success: true,
            message: format!("Created idea: {}", idea.id),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            message: e.to_string(),
        }),
    }
}

// === Reactor API Handlers ===

/// List all features
#[utoipa::path(
    get,
    path = "/api/v1/reactor/features",
    tag = "reactor",
    responses(
        (status = 200, description = "List of features", body = Vec<FeatureResponse>)
    )
)]
async fn list_features(State(state): State<SharedState>) -> Json<Vec<FeatureResponse>> {
    use catalyst_core::state::FeatureManager;

    let fm = FeatureManager::new(&state.db);
    match fm.list_all() {
        Ok(features) => Json(
            features
                .into_iter()
                .map(|f| FeatureResponse {
                    id: f.id,
                    title: f.title,
                    stage: format!("{:?}", f.stage),
                    description: f.description,
                    created_at: f.created_at.to_rfc3339(),
                })
                .collect(),
        ),
        Err(_) => Json(vec![]),
    }
}

/// Delete a feature
#[utoipa::path(
    delete,
    path = "/api/v1/reactor/features/{id}",
    tag = "reactor",
    params(("id" = String, Path, description = "Feature ID")),
    responses(
        (status = 200, description = "Feature deleted", body = ApiResponse)
    )
)]
async fn delete_feature(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse> {
    use catalyst_core::state::FeatureManager;

    let manager = FeatureManager::new(&state.db);
    match manager.delete(&id) {
        Ok(_) => Json(ApiResponse {
            success: true,
            message: format!("Feature {} deleted", id),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            message: e.to_string(),
        }),
    }
}

/// Promote an idea to a feature
#[utoipa::path(
    post,
    path = "/api/v1/reactor/ignite",
    tag = "reactor",
    request_body = IgniteRequest,
    responses(
        (status = 200, description = "Feature ignited", body = IgniteResponse)
    )
)]
async fn ignite_feature(
    State(state): State<SharedState>,
    Json(req): Json<IgniteRequest>,
) -> Json<IgniteResponse> {
    use catalyst_core::state::{ContextManager, FeatureManager};

    let cm = ContextManager::new(&state.db);
    let fm = FeatureManager::new(&state.db);

    // Load idea and create feature
    match cm.load_idea(&req.idea_id) {
        Ok(idea) => match fm.create(&idea.content) {
            Ok(feature) => Json(IgniteResponse {
                success: true,
                feature_id: Some(feature.id),
                error: None,
            }),
            Err(e) => Json(IgniteResponse {
                success: false,
                feature_id: None,
                error: Some(e.to_string()),
            }),
        },
        Err(e) => Json(IgniteResponse {
            success: false,
            feature_id: None,
            error: Some(format!("Idea not found: {}", e)),
        }),
    }
}

// === Snapshot API Handlers ===

/// List all snapshots
#[utoipa::path(
    get,
    path = "/api/v1/reactor/snapshots",
    tag = "reactor",
    responses(
        (status = 200, description = "List of snapshots", body = Vec<SnapshotResponse>)
    )
)]
async fn list_snapshots(State(state): State<SharedState>) -> Json<Vec<SnapshotResponse>> {
    use catalyst_core::state::SnapshotManager;

    let manager = SnapshotManager::new(&state.db);
    match manager.list() {
        Ok(snapshots) => Json(
            snapshots
                .into_iter()
                .map(|s| SnapshotResponse {
                    id: s.id,
                    stage: s.stage,
                    timestamp: s.timestamp.to_rfc3339(),
                    description: s.description,
                })
                .collect(),
        ),
        Err(_) => Json(vec![]),
    }
}

/// Delete a snapshot
#[utoipa::path(
    delete,
    path = "/api/v1/reactor/snapshots/{id}",
    tag = "reactor",
    params(("id" = String, Path, description = "Snapshot ID")),
    responses(
        (status = 200, description = "Snapshot deleted", body = ApiResponse)
    )
)]
async fn delete_snapshot(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<ApiResponse> {
    use catalyst_core::state::SnapshotManager;

    let manager = SnapshotManager::new(&state.db);
    match manager.delete(&id) {
        Ok(deleted) => Json(ApiResponse {
            success: deleted,
            message: if deleted {
                format!("Snapshot {} deleted", id)
            } else {
                "Snapshot not found".to_string()
            },
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            message: e.to_string(),
        }),
    }
}

/// Rollback to a snapshot
#[utoipa::path(
    post,
    path = "/api/v1/reactor/rollback",
    tag = "reactor",
    request_body = RollbackRequest,
    responses(
        (status = 200, description = "Rollback result", body = RollbackResponse)
    )
)]
async fn rollback_to_snapshot(
    State(state): State<SharedState>,
    Json(req): Json<RollbackRequest>,
) -> Json<RollbackResponse> {
    use catalyst_core::state::SnapshotManager;
    use catalyst_core::swarm::{SwarmEvent, SwarmEventKind};

    let manager = SnapshotManager::new(&state.db);
    match manager.restore(&req.snapshot_id) {
        Ok(result) => {
            // Emit StateRestored event
            let _ = state.event_tx.send(
                SwarmEvent::new(SwarmEventKind::StateRestored, "system").with_data(
                    serde_json::json!({
                        "snapshot_id": result.snapshot_id,
                        "stage": result.stage
                    }),
                ),
            );

            Json(RollbackResponse {
                success: true,
                snapshot_id: result.snapshot_id,
                stage: result.stage,
                message: result.message,
            })
        }
        Err(e) => Json(RollbackResponse {
            success: false,
            snapshot_id: req.snapshot_id.clone(),
            stage: "unknown".to_string(),
            message: format!("Rollback failed: {}", e),
        }),
    }
}

// === Inbox API Handlers ===

/// List pending inbox interactions
#[utoipa::path(
    get,
    path = "/api/v1/inbox",
    tag = "inbox",
    responses(
        (status = 200, description = "List of pending interactions", body = InboxListResponse)
    )
)]
async fn list_inbox(State(state): State<SharedState>) -> Json<InboxListResponse> {
    use catalyst_core::state::InteractionManager;

    let manager = InteractionManager::new(&state.db);
    match manager.list_pending() {
        Ok(interactions) => Json(InboxListResponse {
            items: interactions
                .into_iter()
                .map(|i| InboxItem {
                    id: i.id,
                    kind: format!("{:?}", i.kind),
                    from_agent: i.from_agent,
                    title: i.title,
                    description: i.description,
                    options: i.options,
                    created_at: i.created_at.to_rfc3339(),
                })
                .collect(),
        }),
        Err(_) => Json(InboxListResponse { items: vec![] }),
    }
}

/// Reply to an inbox interaction
#[utoipa::path(
    post,
    path = "/api/v1/inbox/{id}/reply",
    tag = "inbox",
    params(
        ("id" = String, Path, description = "Interaction ID")
    ),
    request_body = InboxReplyRequest,
    responses(
        (status = 200, description = "Reply result", body = InboxReplyResponse)
    )
)]
async fn reply_to_inbox(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<InboxReplyRequest>,
) -> Json<InboxReplyResponse> {
    use catalyst_core::state::{InteractionManager, InteractionResponse};
    use catalyst_core::swarm::CoordinatorCommand;

    // 1. Update database
    let manager = InteractionManager::new(&state.db);

    let response = InteractionResponse {
        selected_option: req.selected_option,
        text_input: req.text_input,
        attachments: vec![],
        responded_by: "user".to_string(),
    };

    if let Err(e) = manager.resolve(&id, response) {
        return Json(InboxReplyResponse {
            success: false,
            message: format!("Failed to resolve interaction: {}", e),
        });
    }

    // 2. Wake coordinator if channel available
    if let Some(tx) = state.coordinator_tx.read().await.as_ref() {
        let _ = tx.send(CoordinatorCommand::Resume(id.clone())).await;
    }

    Json(InboxReplyResponse {
        success: true,
        message: "Coordinator resumed".to_string(),
    })
}

/// List resolved inbox interactions (history)
#[utoipa::path(
    get,
    path = "/api/v1/inbox/history",
    tag = "inbox",
    responses(
        (status = 200, description = "List of resolved interactions", body = InboxListResponse)
    )
)]
async fn list_inbox_history(State(state): State<SharedState>) -> Json<InboxListResponse> {
    use catalyst_core::state::InteractionManager;

    let manager = InteractionManager::new(&state.db);
    match manager.list_history(50) {
        Ok(interactions) => Json(InboxListResponse {
            items: interactions
                .into_iter()
                .map(|i| InboxItem {
                    id: i.id,
                    kind: format!("{:?}", i.kind),
                    from_agent: i.from_agent,
                    title: i.title,
                    description: i.description,
                    options: i.options,
                    created_at: i.created_at.to_rfc3339(),
                })
                .collect(),
        }),
        Err(_) => Json(InboxListResponse { items: vec![] }),
    }
}

// === Project API Handlers (Brownfield Init) ===

#[derive(Serialize, ToSchema)]
struct ProjectStatusResponse {
    initialized: bool,
    project_type: Option<String>,
    total_files: Option<u32>,
    total_loc: Option<u32>,
}

/// Get project initialization status
#[utoipa::path(
    get,
    path = "/api/v1/project/status",
    tag = "project",
    responses(
        (status = 200, description = "Project status", body = ProjectStatusResponse)
    )
)]
async fn get_project_status(State(state): State<SharedState>) -> Json<ProjectStatusResponse> {
    use catalyst_core::state::CodebaseProfile;

    if CodebaseProfile::exists(&state.db) {
        match CodebaseProfile::load(&state.db) {
            Ok(profile) => Json(ProjectStatusResponse {
                initialized: true,
                project_type: Some(format!("{:?}", profile.project_type)),
                total_files: Some(profile.total_files),
                total_loc: Some(profile.total_loc),
            }),
            Err(_) => Json(ProjectStatusResponse {
                initialized: false,
                project_type: None,
                total_files: None,
                total_loc: None,
            }),
        }
    } else {
        Json(ProjectStatusResponse {
            initialized: false,
            project_type: None,
            total_files: None,
            total_loc: None,
        })
    }
}

/// Get detailed codebase profile
#[utoipa::path(
    get,
    path = "/api/v1/project/profile",
    tag = "project",
    responses(
        (status = 200, description = "Codebase profile", body = ProfileResponse)
    )
)]
async fn get_project_profile(State(state): State<SharedState>) -> Json<ProfileResponse> {
    use catalyst_core::state::CodebaseProfile;

    match CodebaseProfile::load(&state.db) {
        Ok(profile) => Json(ProfileResponse {
            exists: true,
            profile: Some(CodebaseProfileResponse {
                project_type: format!("{:?}", profile.project_type),
                total_files: profile.total_files,
                total_loc: profile.total_loc,
                frameworks: profile.frameworks.iter().map(|f| f.name.clone()).collect(),
                style_patterns: StylePatternsResponse {
                    naming_convention: profile.style_patterns.naming_convention.clone(),
                    error_handling: profile.style_patterns.error_handling.clone(),
                    async_runtime: profile.style_patterns.async_runtime.clone(),
                    test_framework: profile.style_patterns.test_framework.clone(),
                },
                scanned_at: profile.scanned_at.to_rfc3339(),
            }),
        }),
        Err(_) => Json(ProfileResponse {
            exists: false,
            profile: None,
        }),
    }
}

#[derive(Deserialize, ToSchema)]
struct InitProjectRequest {
    source: String,
    path: String,
    mode: String,
}

#[derive(Serialize, ToSchema)]
struct InitProjectResponse {
    success: bool,
    project_type: String,
    total_files: u32,
    total_loc: u32,
    frameworks: Vec<String>,
    message: String,
}

/// Initialize a project
#[utoipa::path(
    post,
    path = "/api/v1/project/init",
    tag = "project",
    request_body = InitProjectRequest,
    responses(
        (status = 200, description = "Project initialized", body = InitProjectResponse)
    )
)]
async fn init_project(
    State(state): State<SharedState>,
    Json(req): Json<InitProjectRequest>,
) -> Json<InitProjectResponse> {
    use catalyst_core::swarm::initialize_project;
    use std::path::PathBuf;

    let path = match req.source.as_str() {
        "local" => PathBuf::from(&req.path),
        "current" => std::env::current_dir().unwrap_or_default(),
        _ => {
            return Json(InitProjectResponse {
                success: false,
                project_type: "unknown".to_string(),
                total_files: 0,
                total_loc: 0,
                frameworks: vec![],
                message: format!("Unknown source type: {}", req.source),
            });
        }
    };

    match initialize_project(&path, &req.mode, &state.db, None).await {
        Ok(profile) => Json(InitProjectResponse {
            success: true,
            project_type: format!("{:?}", profile.project_type),
            total_files: profile.total_files,
            total_loc: profile.total_loc,
            frameworks: profile.frameworks.iter().map(|f| f.name.clone()).collect(),
            message: "Project initialized successfully".to_string(),
        }),
        Err(e) => Json(InitProjectResponse {
            success: false,
            project_type: "unknown".to_string(),
            total_files: 0,
            total_loc: 0,
            frameworks: vec![],
            message: format!("Initialization failed: {}", e),
        }),
    }
}

// === Prompt Template Handlers ===

/// List all prompts
#[utoipa::path(
    get,
    path = "/api/v1/prompts",
    tag = "prompts",
    responses(
        (status = 200, description = "List of prompts", body = PromptListResponse)
    )
)]
async fn list_prompts(State(state): State<SharedState>) -> Json<PromptListResponse> {
    match state.db.list_prompts() {
        Ok(prompts) => Json(PromptListResponse {
            prompts: prompts
                .into_iter()
                .map(|(slug, version)| PromptListItem { slug, version })
                .collect(),
        }),
        Err(_) => Json(PromptListResponse { prompts: vec![] }),
    }
}

/// Get a prompt by slug
#[utoipa::path(
    get,
    path = "/api/v1/prompts/{slug}",
    tag = "prompts",
    params(("slug" = String, Path, description = "Prompt slug")),
    responses(
        (status = 200, description = "Prompt content", body = PromptResponse)
    )
)]
async fn get_prompt(
    State(state): State<SharedState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Json<PromptResponse> {
    match state.db.get_prompt_versioned(&slug) {
        Ok((content, version)) => Json(PromptResponse {
            slug,
            content,
            version,
        }),
        Err(_) => Json(PromptResponse {
            slug,
            content: "".to_string(),
            version: 0,
        }),
    }
}

/// Update a prompt
#[utoipa::path(
    put,
    path = "/api/v1/prompts/{slug}",
    tag = "prompts",
    params(("slug" = String, Path, description = "Prompt slug")),
    request_body = UpdatePromptRequest,
    responses(
        (status = 200, description = "Prompt updated", body = UpdatePromptResponse)
    )
)]
async fn update_prompt(
    State(state): State<SharedState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(req): Json<UpdatePromptRequest>,
) -> Json<UpdatePromptResponse> {
    match state.db.set_prompt(&slug, &req.content) {
        Ok(new_version) => Json(UpdatePromptResponse {
            success: true,
            slug,
            new_version,
        }),
        Err(_) => Json(UpdatePromptResponse {
            success: false,
            slug,
            new_version: 0,
        }),
    }
}

// === Project Document Handlers ===

/// List all documents
#[utoipa::path(
    get,
    path = "/api/v1/documents",
    tag = "documents",
    responses(
        (status = 200, description = "List of documents", body = DocumentListResponse)
    )
)]
async fn list_documents(State(state): State<SharedState>) -> Json<DocumentListResponse> {
    match state.db.list_documents() {
        Ok(docs) => Json(DocumentListResponse { documents: docs }),
        Err(_) => Json(DocumentListResponse { documents: vec![] }),
    }
}

/// Get a document by slug
#[utoipa::path(
    get,
    path = "/api/v1/documents/{slug}",
    tag = "documents",
    params(("slug" = String, Path, description = "Document slug")),
    responses(
        (status = 200, description = "Document content", body = DocumentResponse)
    )
)]
async fn get_document(
    State(state): State<SharedState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Json<DocumentResponse> {
    match state.db.get_document(&slug) {
        Ok((title, content)) => Json(DocumentResponse {
            slug,
            title,
            content,
        }),
        Err(_) => Json(DocumentResponse {
            slug,
            title: "".to_string(),
            content: "".to_string(),
        }),
    }
}

/// Update a document
#[utoipa::path(
    put,
    path = "/api/v1/documents/{slug}",
    tag = "documents",
    params(("slug" = String, Path, description = "Document slug")),
    request_body = UpdateDocumentRequest,
    responses(
        (status = 200, description = "Document saved", body = ApiResponse)
    )
)]
async fn update_document(
    State(state): State<SharedState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Json(req): Json<UpdateDocumentRequest>,
) -> Json<ApiResponse> {
    match state.db.set_document(&slug, &req.title, &req.content) {
        Ok(_) => Json(ApiResponse {
            success: true,
            message: "Document saved".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            success: false,
            message: e.to_string(),
        }),
    }
}

// === PTY WebSocket Handler ===

async fn pty_websocket(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_pty_socket)
}

async fn handle_pty_socket(socket: WebSocket) {
    use std::io::Read;

    let (mut sender, mut receiver) = socket.split();

    // Spawn PTY with shell
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("Failed to open PTY: {}", e);
            let _ = sender.send(Message::Text(format!("Error: {}", e))).await;
            return;
        }
    };

    // Determine shell based on OS
    #[cfg(windows)]
    let shell = "powershell.exe";
    #[cfg(not(windows))]
    let shell = "/bin/bash";

    let mut cmd = CommandBuilder::new(shell);
    cmd.cwd(std::env::current_dir().unwrap_or_default());

    let child = match pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Failed to spawn shell: {}", e);
            let _ = sender.send(Message::Text(format!("Error: {}", e))).await;
            return;
        }
    };

    // Get reader/writer for the PTY master
    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = pair.master.take_writer().unwrap();

    // Task to read from PTY and send to WebSocket
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);

    // Spawn blocking reader for PTY output
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Forward PTY output to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if sender.send(Message::Binary(data)).await.is_err() {
                break;
            }
        }
    });

    // Forward WebSocket input to PTY
    let writer = std::sync::Arc::new(std::sync::Mutex::new(writer));
    let writer_clone = writer.clone();

    let recv_task = tokio::spawn(async move {
        use std::io::Write;
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(mut w) = writer_clone.lock() {
                        if w.write_all(text.as_bytes()).is_err() {
                            break;
                        }
                        let _ = w.flush();
                    }
                }
                Message::Binary(data) => {
                    if let Ok(mut w) = writer_clone.lock() {
                        if w.write_all(&data).is_err() {
                            break;
                        }
                        let _ = w.flush();
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    // Child process will be dropped and killed when we exit
    drop(child);
}

// === OpenAPI Handler ===

async fn serve_openapi() -> impl IntoResponse {
    let spec = ApiDoc::openapi().to_json().unwrap_or_default();
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(spec))
        .unwrap()
}

// === A2A Agent Card Handler ===

/// Serve the A2A agent card for agent discovery
/// Returns JSON conformant with the A2A protocol specification
///
/// Note: AgentDefinition fields are private in radkit, so we define the
/// agent card statically based on our known skill metadata.
async fn serve_agent_card() -> impl IntoResponse {
    // Build A2A-compliant agent card with static skill metadata
    let agent_card = serde_json::json!({
        "name": "Catalyst Swarm",
        "description": "Autonomous coding agent swarm with compile-time intelligence and runtime execution.",
        "url": "http://localhost:8080",
        "provider": {
            "organization": "Catalyst",
            "url": "https://github.com/catalyst"
        },
        "version": "1.0.0",
        "capabilities": {
            "streaming": true,
            "pushNotifications": false,
            "stateTransitionHistory": false
        },
        "skills": [
            {
                "id": "orchestrator",
                "name": "Pipeline Orchestrator",
                "description": "Coordinates the agent pipeline from user goal to completed feature. Manages state transitions: Parse → Research → Architect → Critic → Atomize → Build.",
                "tags": ["orchestration", "pipeline", "meta-agent"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "parse",
                "name": "Parse Unknowns",
                "description": "Parses user goals and identifies ambiguities that must be resolved before code generation.",
                "tags": ["parsing", "analysis", "compile-time"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "research",
                "name": "Research Agent",
                "description": "Researches solutions for unknowns using web search and crate search.",
                "tags": ["research", "search", "analysis"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "architect",
                "name": "Architect",
                "description": "Makes architectural decisions based on research and project context.",
                "tags": ["architecture", "design", "decisions"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "review",
                "name": "Critic",
                "description": "Reviews architectural decisions and code changes for quality and correctness.",
                "tags": ["review", "quality", "feedback"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "atomize",
                "name": "Atomizer",
                "description": "Breaks features into agent-sized modules following the Rule of 100.",
                "tags": ["planning", "modular", "compile-time"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "taskmaster",
                "name": "Taskmaster",
                "description": "Bundles context into mission prompts for coding agents.",
                "tags": ["mission", "context", "bridge"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "implement",
                "name": "Builder",
                "description": "Implements features in code. Reads files, makes changes, runs builds to verify.",
                "tags": ["coding", "rust", "implementation"],
                "inputModes": ["text/plain", "application/json"],
                "outputModes": ["application/json"]
            },
            {
                "id": "scrape",
                "name": "WebScraper",
                "description": "Cleans HTML into plain text for research. Extracts key points and filters irrelevant content.",
                "tags": ["scraping", "html", "utility"],
                "inputModes": ["text/plain", "text/html", "application/json"],
                "outputModes": ["application/json"]
            }
        ]
    });

    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(
            serde_json::to_string_pretty(&agent_card).unwrap_or_default(),
        ))
        .unwrap()
}

// === Config Handlers ===

/// Get current configuration
#[utoipa::path(
    get,
    path = "/api/v1/config",
    tag = "config",
    responses(
        (status = 200, description = "Current configuration and defaults", body = ConfigResponse)
    )
)]
async fn get_config() -> Json<ConfigResponse> {
    let config = PersistedConfig::load().await;
    Json(ConfigResponse {
        config,
        defaults: ConfigDefaults::default(),
    })
}

/// Update configuration (partial merge)
#[utoipa::path(
    patch,
    path = "/api/v1/config",
    tag = "config",
    request_body = PersistedConfig,
    responses(
        (status = 200, description = "Updated configuration", body = ConfigResponse)
    )
)]
async fn update_config(Json(updates): Json<PersistedConfig>) -> Json<ConfigResponse> {
    let mut config = PersistedConfig::load().await;
    config.merge(updates);

    if let Err(e) = config.save().await {
        eprintln!("Failed to save config: {}", e);
    }

    // Update env var for search_tools to pick up
    if let Some(ref url) = config.searxng_url {
        std::env::set_var("SEARXNG_URL", url);
    }

    Json(ConfigResponse {
        config,
        defaults: ConfigDefaults::default(),
    })
}

/// Get available LLM providers
#[utoipa::path(
    get,
    path = "/api/v1/providers",
    tag = "providers",
    responses(
        (status = 200, description = "List of supported LLM providers", body = ProvidersResponse)
    )
)]
async fn get_providers() -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        providers: get_provider_info(),
    })
}

// === Static File Serving ===

async fn proxy_frontend(uri: Uri) -> impl IntoResponse {
    let client = reqwest::Client::new();
    let dev_server_url = "http://localhost:5173";
    let url = format!("{}{}", dev_server_url, uri.path());

    // Check if query params exist (Vite HMR uses them)
    // Note: This is a simple proxy. For HMR, we might need WebSocket forwarding
    // which reqwest doesn't do perfectly, but for loading assets it's fine.
    // For now, let's just forward GET requests.

    match client.get(&url).send().await {
        Ok(res) => {
            let mut response = Response::builder().status(res.status());

            // Copy headers
            for (key, value) in res.headers() {
                response = response.header(key, value);
            }

            response
                .body(Body::from_stream(res.bytes_stream()))
                .unwrap()
        }
        Err(_) => Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::from("Vite server not ready?"))
            .unwrap(),
    }
}

async fn serve_static(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(file.data.to_vec()))
            .unwrap();
    }

    // SPA fallback
    if let Some(file) = Assets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(file.data.to_vec()))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .unwrap()
}

// === Server Entry ===

pub async fn run_server() -> anyhow::Result<()> {
    let (event_tx, _) = broadcast::channel::<SwarmEvent>(100);

    // Initialize the unified database
    let db = Arc::new(CatalystDb::open().expect("Failed to open CatalystDb"));

    // Seed default prompts and project documents
    match db.seed_prompts() {
        Ok(count) if count > 0 => println!("📝 Seeded {} default prompts", count),
        Ok(_) => {} // Already seeded
        Err(e) => eprintln!("⚠️ Failed to seed prompts: {}", e),
    }

    // Initialize memory service with the shared database
    let memory = CatalystMemory::new_with_db(&db, MemoryConfig::default());

    // Index project documents from database into memory
    match memory.index_project_docs(&db).await {
        Ok(count) => println!("📚 Indexed {} documents into memory", count),
        Err(e) => eprintln!("⚠️ Failed to index documents: {}", e),
    }

    let state: SharedState = Arc::new(AppState {
        swarm_status: RwLock::new(SwarmStatus::default()),
        event_tx,
        memory,
        db,
        pending_approvals: RwLock::new(HashMap::new()),
        coordinator_tx: RwLock::new(None),
    });

    let swarm_routes = Router::new()
        .route("/status", get(get_status))
        .route("/start", post(start_swarm))
        .route("/stop", post(stop_swarm))
        .route("/approve", post(handle_approval))
        .route("/events", get(events));

    let memory_routes = Router::new().route("/search", post(search_memory));

    // Braindump routes (Factory Tab)
    let braindump_routes = Router::new()
        .route("/", get(list_braindump))
        .route("/ideas", post(create_idea));

    // Reactor routes (Factory Tab)
    let reactor_routes = Router::new()
        .route("/features", get(list_features))
        .route("/features/:id", delete(delete_feature))
        .route("/ignite", post(ignite_feature))
        .route("/snapshots", get(list_snapshots))
        .route("/snapshots/:id", delete(delete_snapshot))
        .route("/rollback", post(rollback_to_snapshot));

    // Inbox routes (human-in-the-loop)
    let inbox_routes = Router::new()
        .route("/", get(list_inbox))
        .route("/:id/reply", post(reply_to_inbox))
        .route("/history", get(list_inbox_history));

    // Project routes (brownfield init)
    let project_routes = Router::new()
        .route("/status", get(get_project_status))
        .route("/profile", get(get_project_profile))
        .route("/init", post(init_project));

    // Prompt template routes
    let prompt_routes = Router::new()
        .route("/", get(list_prompts))
        .route("/:slug", get(get_prompt).put(update_prompt));

    // Project document routes
    let document_routes = Router::new()
        .route("/", get(list_documents))
        .route("/:slug", get(get_document).put(update_document));

    let args = Args::parse();

    // Handle subcommands and determine port
    let server_port = match args.command {
        Some(CliCommand::Init { name, description }) => {
            // Initialize a new Catalyst project
            println!("🔧 Initializing Catalyst project...");

            use std::fs;
            let catalyst_dir = std::path::Path::new(".catalyst");

            // Create .catalyst directory
            if let Err(e) = fs::create_dir_all(catalyst_dir) {
                eprintln!("❌ Failed to create .catalyst directory: {}", e);
                return Ok(());
            }

            // Create .gitignore
            let gitignore = catalyst_dir.join(".gitignore");
            let _ = fs::write(
                &gitignore,
                "# Never commit secrets\n.env\n*.env\nstate.json\n",
            );

            // Create initial state.json
            let state_json = catalyst_dir.join("state.json");
            let state_content = serde_json::json!({
                "project_name": name.clone().unwrap_or_else(|| "My Project".to_string()),
                "description": description.clone().unwrap_or_else(|| "A Catalyst-powered project".to_string()),
                "status": "initialized"
            });
            if let Err(e) = fs::write(
                &state_json,
                serde_json::to_string_pretty(&state_content).unwrap(),
            ) {
                eprintln!("❌ Failed to write state.json: {}", e);
                return Ok(());
            }

            // Create spec.md template
            let spec_md = std::path::Path::new("spec.md");
            if !spec_md.exists() {
                let spec_template = format!(
                    "# {}\n\n{}\n\n## Features\n\n- [ ] Feature 1\n\n## Technical Details\n\nAdd technical requirements here.\n",
                    name.clone().unwrap_or_else(|| "Project Name".to_string()),
                    description.clone().unwrap_or_else(|| "Project description".to_string())
                );
                let _ = fs::write(spec_md, spec_template);
            }

            println!("✅ Catalyst project initialized!");
            println!("   Created: .catalyst/");
            println!("   Created: .catalyst/.gitignore");
            println!("   Created: .catalyst/state.json");
            if !std::path::Path::new("spec.md").exists() {
                println!("   Created: spec.md");
            }
            println!("\n🚀 Run `catalyst serve` to start the server");
            return Ok(());
        }
        Some(CliCommand::Run { goal }) => {
            // Run swarm directly without server
            println!("🚀 Running swarm with goal: {}", goal);
            let db = Arc::new(CatalystDb::open().expect("Failed to open CatalystDb"));
            let config = CoordinatorConfig::default();
            let mut coordinator = Coordinator::new(config, db).with_research_agent();
            match coordinator.run(&goal).await {
                Ok(result) => {
                    println!("✅ Swarm completed! Success: {}", result.success);
                    println!("   Decisions: {}", result.decisions.len());
                }
                Err(e) => {
                    eprintln!("❌ Swarm failed: {}", e);
                }
            }
            return Ok(());
        }
        Some(CliCommand::Serve { dev, port }) => {
            // Start server with explicit dev mode and port
            if dev {
                println!("🔧 Starting in DEV MODE");
                let frontend_dir = "apps/frontend";
                println!("   ⚡ Spawning pnpm dev in {}", frontend_dir);
                let _child = tokio::process::Command::new("cmd")
                    .args(&["/C", "pnpm", "dev"])
                    .current_dir(frontend_dir)
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn();
            }
            port // Use specified port
        }
        None => {
            8080 // Default port
        }
    };

    // Settings routes (API key management)
    let settings_routes = Router::new().route("/api-keys", post(save_api_keys));

    // Determine dev mode for fallback
    let dev_mode = matches!(args.command, Some(CliCommand::Serve { dev: true, .. }));

    let app = Router::new()
        // v1 API routes
        .nest("/api/v1/swarm", swarm_routes)
        .nest("/api/v1/memory", memory_routes)
        .nest("/api/v1/braindump", braindump_routes)
        .nest("/api/v1/reactor", reactor_routes)
        .nest("/api/v1/inbox", inbox_routes)
        .nest("/api/v1/project", project_routes)
        .nest("/api/v1/settings", settings_routes)
        .nest("/api/v1/prompts", prompt_routes)
        .nest("/api/v1/documents", document_routes)
        .route("/api/v1/config", get(get_config).patch(update_config))
        .route("/api/v1/providers", get(get_providers))
        .route("/api/v1/openapi.json", get(serve_openapi))
        // A2A Discovery endpoint
        .route("/.well-known/agent-card.json", get(serve_agent_card))
        // Non-versioned routes (WebSocket)
        .route("/api/pty", get(pty_websocket));

    let app = if dev_mode {
        app.fallback(any(proxy_frontend))
    } else {
        app.fallback(get(serve_static))
    };

    let app = app.with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], server_port));
    println!("🚀 Catalyst Server running at http://{}", addr);
    println!("   API v1 Routes:");
    println!("   Swarm:     /api/v1/swarm/status, /start, /events");
    println!("   Memory:    /api/v1/memory/search");
    println!("   Braindump: /api/v1/braindump, /ideas");
    println!("   Reactor:   /api/v1/reactor/features, /ignite");
    println!("   Project:   /api/v1/project/status, /init");
    println!("   Config:    /api/v1/config (GET, PATCH)");
    println!("   Providers: /api/v1/providers (GET)");
    println!("   Terminal:  /api/pty (WebSocket)");

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════╗");
    println!("║         CATALYST SERVER              ║");
    println!("╚══════════════════════════════════════╝");

    run_server().await
}
