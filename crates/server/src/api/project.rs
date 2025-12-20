//! # Project API
//!
//! Endpoints for project initialization and brownfield onboarding.

use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::PathBuf;
use tokio::sync::mpsc;

use catalyst_core::state::CodebaseProfile;
use catalyst_core::swarm::{initialize_project, ScanProgress};

use crate::AppState;

/// Request to initialize a project
#[derive(Debug, Deserialize)]
pub struct InitRequest {
    /// Source type: "local" or "github"
    pub source: String,
    /// Path or URL
    pub path: String,
    /// Operating mode
    pub mode: String,
}

/// Response after initialization
#[derive(Debug, Serialize)]
pub struct InitResponse {
    pub success: bool,
    pub project_type: String,
    pub total_files: u32,
    pub total_loc: u32,
    pub frameworks: Vec<String>,
    pub message: String,
}

/// Status check response
#[derive(Debug, Serialize)]
pub struct ProjectStatus {
    pub initialized: bool,
    pub project_type: Option<String>,
    pub has_profile: bool,
}

pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/init", post(init_project))
        .route("/init/progress", get(init_progress_sse))
}

/// Check if project is initialized
async fn get_status() -> Json<ProjectStatus> {
    let has_profile = CodebaseProfile::exists();

    // Try to load profile for project type
    let project_type = if has_profile {
        CodebaseProfile::load()
            .await
            .ok()
            .map(|p| format!("{:?}", p.project_type))
    } else {
        None
    };

    Json(ProjectStatus {
        initialized: has_profile,
        project_type,
        has_profile,
    })
}

/// Initialize project (start scanning)
async fn init_project(
    State(state): State<AppState>,
    Json(req): Json<InitRequest>,
) -> Json<InitResponse> {
    let path = match req.source.as_str() {
        "local" => PathBuf::from(&req.path),
        "github" => {
            // TODO: Clone repo first
            return Json(InitResponse {
                success: false,
                project_type: "unknown".to_string(),
                total_files: 0,
                total_loc: 0,
                frameworks: vec![],
                message: "GitHub clone not yet implemented".to_string(),
            });
        }
        _ => {
            return Json(InitResponse {
                success: false,
                project_type: "unknown".to_string(),
                total_files: 0,
                total_loc: 0,
                frameworks: vec![],
                message: format!("Unknown source type: {}", req.source),
            });
        }
    };

    // Create progress channel
    let (tx, mut _rx) = mpsc::channel::<ScanProgress>(32);

    // Run initialization
    match initialize_project(&path, &req.mode, Some(tx)).await {
        Ok(profile) => Json(InitResponse {
            success: true,
            project_type: format!("{:?}", profile.project_type),
            total_files: profile.total_files,
            total_loc: profile.total_loc,
            frameworks: profile.frameworks.iter().map(|f| f.name.clone()).collect(),
            message: "Project initialized successfully".to_string(),
        }),
        Err(e) => Json(InitResponse {
            success: false,
            project_type: "unknown".to_string(),
            total_files: 0,
            total_loc: 0,
            frameworks: vec![],
            message: format!("Initialization failed: {}", e),
        }),
    }
}

/// SSE endpoint for scan progress
async fn init_progress_sse() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // This would connect to the actual scan progress channel
    // For now, return a simple stream
    let stream = futures::stream::iter(vec![Ok(
        Event::default().data(r#"{"phase":"Starting","progress":0}"#)
    )]);

    Sse::new(stream)
}
