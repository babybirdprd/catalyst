//! # Project Initialization
//!
//! Handles brownfield project onboarding - scanning existing codebases
//! and generating profiles for agent context.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::state::codebase_profile::{ApiSignature, FrameworkInfo, NamingConventions};
use crate::state::{CatalystDb, CodebaseProfile, ProjectType, StylePatterns};
use crate::swarm::architecture_generator;
use crate::tools::{ast_scanner, scanner};

/// Progress update during scanning
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub phase: String,
    pub progress: u8,
    pub files_found: u32,
    pub loc: u32,
    pub message: String,
}

/// Result of project detection
#[derive(Debug)]
pub struct ProjectDetection {
    pub project_type: ProjectType,
    pub has_cargo_toml: bool,
    pub has_package_json: bool,
    pub has_pyproject: bool,
}

/// Detect project type from root directory
pub fn detect_project(root: &Path) -> ProjectDetection {
    let has_cargo = root.join("Cargo.toml").exists();
    let has_package = root.join("package.json").exists();
    let has_py = root.join("pyproject.toml").exists() || root.join("setup.py").exists();

    let project_type = match (has_cargo, has_package, has_py) {
        (true, false, false) => ProjectType::Rust,
        (false, true, false) => ProjectType::TypeScript,
        (false, false, true) => ProjectType::Python,
        (true, true, _) => ProjectType::Mixed,
        _ => ProjectType::Unknown,
    };

    ProjectDetection {
        project_type,
        has_cargo_toml: has_cargo,
        has_package_json: has_package,
        has_pyproject: has_py,
    }
}

/// Scan a Rust project and generate profile
pub async fn scan_rust_project(
    root: &Path,
    progress_tx: Option<mpsc::Sender<ScanProgress>>,
) -> Result<CodebaseProfile> {
    let mut profile = CodebaseProfile::new(root.to_path_buf());
    profile.project_type = ProjectType::Rust;

    // Phase 1: Count files and LOC
    send_progress(
        &progress_tx,
        "Counting files",
        10,
        0,
        0,
        "Scanning directory...",
    )
    .await;

    let (file_count, total_loc) = count_rust_files(root)?;
    profile.total_files = file_count;
    profile.total_loc = total_loc;

    send_progress(
        &progress_tx,
        "Counting files",
        25,
        file_count,
        total_loc,
        &format!("Found {} files, {} LOC", file_count, total_loc),
    )
    .await;

    // Phase 2: Detect frameworks from Cargo.toml
    send_progress(
        &progress_tx,
        "Detecting frameworks",
        35,
        file_count,
        total_loc,
        "Reading Cargo.toml...",
    )
    .await;

    profile.frameworks = detect_rust_frameworks(root)?;

    // Phase 3: Extract public APIs
    send_progress(
        &progress_tx,
        "Extracting APIs",
        50,
        file_count,
        total_loc,
        "Parsing source files...",
    )
    .await;

    let apis = scanner::extract_rust_signatures(root).await?;
    profile.public_apis = apis
        .into_iter()
        .map(|sig| ApiSignature {
            name: sig.name,
            kind: sig.kind,
            signature: sig.signature,
            file: sig.file,
            line: sig.line,
        })
        .collect();

    send_progress(
        &progress_tx,
        "Extracting APIs",
        70,
        file_count,
        total_loc,
        &format!("Found {} public APIs", profile.public_apis.len()),
    )
    .await;

    // Phase 4: Detect style patterns
    send_progress(
        &progress_tx,
        "Analyzing patterns",
        85,
        file_count,
        total_loc,
        "Detecting code style...",
    )
    .await;

    profile.style_patterns = detect_rust_patterns(root, &profile.frameworks);

    // Phase 5: Generate architecture.md with tree-sitter semantic analysis
    send_progress(
        &progress_tx,
        "Generating architecture",
        90,
        file_count,
        total_loc,
        "Building semantic map...",
    )
    .await;

    // Build semantic map using tree-sitter
    let semantic_map = ast_scanner::build_semantic_map(root).await?;

    // Detect naming conventions from semantic map
    profile.naming_conventions = detect_naming_conventions(&semantic_map);

    // Generate architecture.md
    let arch_md = architecture_generator::generate_architecture(&profile, &semantic_map)?;
    crate::state::io::write_runtime_file("context/architecture.md", &arch_md).await?;

    // Phase 6: Complete
    send_progress(
        &progress_tx,
        "Complete",
        100,
        file_count,
        total_loc,
        "Scan complete!",
    )
    .await;

    Ok(profile)
}

/// Count Rust source files and total LOC
fn count_rust_files(root: &Path) -> Result<(u32, u32)> {
    use walkdir::WalkDir;

    let mut file_count = 0u32;
    let mut total_loc = 0u32;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && name != "target" && name != "node_modules"
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "rs" {
                    file_count += 1;
                    if let Ok(content) = std::fs::read_to_string(path) {
                        total_loc += content.lines().count() as u32;
                    }
                }
            }
        }
    }

    Ok((file_count, total_loc))
}

/// Detect frameworks from Cargo.toml dependencies
fn detect_rust_frameworks(root: &Path) -> Result<Vec<FrameworkInfo>> {
    let cargo_toml = root.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml).context("Failed to read Cargo.toml")?;

    let mut frameworks = Vec::new();

    // Simple pattern matching for common frameworks
    let patterns = [
        ("axum", "web"),
        ("actix-web", "web"),
        ("rocket", "web"),
        ("warp", "web"),
        ("sqlx", "database"),
        ("diesel", "database"),
        ("sea-orm", "database"),
        ("tokio", "async"),
        ("async-std", "async"),
        ("serde", "serialization"),
        ("tracing", "logging"),
        ("anyhow", "error"),
        ("thiserror", "error"),
    ];

    for (name, category) in patterns {
        if content.contains(&format!("\"{}\"", name)) || content.contains(&format!("{} =", name)) {
            frameworks.push(FrameworkInfo {
                name: name.to_string(),
                version: None, // Could parse version
                category: category.to_string(),
            });
        }
    }

    Ok(frameworks)
}

/// Detect code style patterns
fn detect_rust_patterns(root: &Path, frameworks: &[FrameworkInfo]) -> StylePatterns {
    let mut patterns = StylePatterns::default();

    // Naming: Rust is always snake_case
    patterns.naming_convention = Some("snake_case".to_string());

    // Error handling
    let has_anyhow = frameworks.iter().any(|f| f.name == "anyhow");
    let has_thiserror = frameworks.iter().any(|f| f.name == "thiserror");
    patterns.error_handling = Some(
        match (has_anyhow, has_thiserror) {
            (true, true) => "anyhow + thiserror",
            (true, false) => "anyhow",
            (false, true) => "thiserror",
            _ => "Result<T, E>",
        }
        .to_string(),
    );

    // Async runtime
    let has_tokio = frameworks.iter().any(|f| f.name == "tokio");
    let has_async_std = frameworks.iter().any(|f| f.name == "async-std");
    patterns.async_runtime = Some(
        match (has_tokio, has_async_std) {
            (true, _) => "tokio",
            (_, true) => "async-std",
            _ => "sync",
        }
        .to_string(),
    );

    // Logging
    let has_tracing = frameworks.iter().any(|f| f.name == "tracing");
    if has_tracing {
        patterns.logging = Some("tracing".to_string());
    }

    patterns
}

/// Detect naming conventions from semantic map
fn detect_naming_conventions(semantic: &ast_scanner::SemanticMap) -> NamingConventions {
    let mut conventions = NamingConventions::default();

    // Rust is always snake_case for functions
    if !semantic.functions.is_empty() {
        conventions.functions = Some("snake_case".to_string());
    }

    // Structs are PascalCase
    if !semantic.structs.is_empty() {
        conventions.structs = Some("PascalCase".to_string());
    }

    // Modules are snake_case
    if !semantic.modules.is_empty() {
        conventions.modules = Some("snake_case".to_string());
    }

    // Detect error patterns from impl blocks
    let mut error_patterns = Vec::new();

    // Check for Result return types
    let has_result_returns = semantic.functions.iter().any(|f| {
        f.return_type
            .as_ref()
            .map_or(false, |r| r.contains("Result"))
    });
    if has_result_returns {
        error_patterns.push("Result<T, E>".to_string());
    }

    // Check for anyhow::Result usage
    let has_anyhow = semantic.functions.iter().any(|f| {
        f.return_type
            .as_ref()
            .map_or(false, |r| r.contains("anyhow"))
    });
    if has_anyhow {
        error_patterns.push("anyhow::Result".to_string());
    }

    // Check for custom error types via thiserror
    let has_error_structs = semantic
        .structs
        .iter()
        .any(|s| s.name.ends_with("Error") || s.derives.iter().any(|d| d.contains("Error")));
    if has_error_structs {
        error_patterns.push("custom error types".to_string());
    }

    conventions.error_patterns = error_patterns;

    conventions
}

/// Initialize a project (create .catalyst/ with profile)
pub async fn initialize_project(
    root: &Path,
    mode: &str,
    db: &CatalystDb,
    progress_tx: Option<mpsc::Sender<ScanProgress>>,
) -> Result<CodebaseProfile> {
    // Detect project type
    let detection = detect_project(root);

    // Scan based on type
    let profile = match detection.project_type {
        ProjectType::Rust | ProjectType::Mixed => scan_rust_project(root, progress_tx).await?,
        _ => {
            // For other types, create minimal profile
            let mut profile = CodebaseProfile::new(root.to_path_buf());
            profile.project_type = detection.project_type;
            profile
        }
    };

    // Save profile
    profile.save(db)?;

    // Update state.json with brownfield marker
    update_state_for_brownfield(mode).await?;

    Ok(profile)
}

/// Update state.json to mark as brownfield project
async fn update_state_for_brownfield(mode: &str) -> Result<()> {
    use crate::state::io;

    // Read current state or create new
    let state_path = "state.json";
    let mut state: serde_json::Value = if let Ok(content) = io::read_file(state_path).await {
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    // Add brownfield markers
    state["project_type"] = serde_json::json!("brownfield");
    state["mode"] = serde_json::json!(mode);
    state["initialized_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());

    let content = serde_json::to_string_pretty(&state)?;
    io::write_runtime_file(state_path, &content).await?;

    Ok(())
}

/// Helper to send progress updates
async fn send_progress(
    tx: &Option<mpsc::Sender<ScanProgress>>,
    phase: &str,
    progress: u8,
    files: u32,
    loc: u32,
    message: &str,
) {
    if let Some(tx) = tx {
        let _ = tx
            .send(ScanProgress {
                phase: phase.to_string(),
                progress,
                files_found: files,
                loc,
                message: message.to_string(),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_project_rust() {
        // Would need temp dir with Cargo.toml for real test
        let detection = detect_project(&PathBuf::from("."));
        // Current project is Rust
        assert!(matches!(
            detection.project_type,
            ProjectType::Rust | ProjectType::Mixed
        ));
    }
}
