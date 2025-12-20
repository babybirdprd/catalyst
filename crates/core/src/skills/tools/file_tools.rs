//! # File Tools
//!
//! Tools for reading and writing files in the worktree sandbox.
//! Uses ToolContext to maintain worktree_path state across calls.

use radkit::macros::tool;
use radkit::tools::ToolResult;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

/// Arguments for reading a file
#[derive(Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    /// Relative path to the file within the worktree
    pub path: String,
}

/// Read a file from the worktree sandbox
#[tool(
    description = "Read a file's contents from the worktree. Returns the content and line count."
)]
pub async fn read_file(args: ReadFileArgs, ctx: &ToolContext<'_>) -> ToolResult {
    // Get worktree path from context state (set by the skill before calling)
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let full_path = worktree.join(&args.path);

    // Security: ensure path doesn't escape worktree
    if !full_path.starts_with(&worktree) {
        return ToolResult::error("Path escapes worktree sandbox");
    }

    match std::fs::read_to_string(&full_path) {
        Ok(content) => ToolResult::success(json!({
            "path": args.path,
            "content": content,
            "lines": content.lines().count()
        })),
        Err(e) => ToolResult::error(format!("Failed to read '{}': {}", args.path, e)),
    }
}

/// Arguments for writing a file
#[derive(Deserialize, JsonSchema)]
pub struct WriteFileArgs {
    /// Relative path to the file within the worktree
    pub path: String,
    /// Content to write to the file
    pub content: String,
}

/// Write content to a file in the worktree sandbox
#[tool(description = "Write content to a file. Creates parent directories if needed.")]
pub async fn write_file(args: WriteFileArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let full_path = worktree.join(&args.path);

    // Security: ensure path doesn't escape worktree
    if !full_path.starts_with(&worktree) {
        return ToolResult::error("Path escapes worktree sandbox");
    }

    // Create parent directories
    if let Some(parent) = full_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ToolResult::error(format!("Failed to create directories: {}", e));
        }
    }

    match std::fs::write(&full_path, &args.content) {
        Ok(_) => ToolResult::success(json!({
            "path": args.path,
            "bytes_written": args.content.len()
        })),
        Err(e) => ToolResult::error(format!("Failed to write '{}': {}", args.path, e)),
    }
}

/// Arguments for listing directory contents
#[derive(Deserialize, JsonSchema)]
pub struct ListDirArgs {
    /// Relative path to the directory (empty for root)
    pub path: Option<String>,
}

/// List files in a directory
#[tool(description = "List files and subdirectories in a directory.")]
pub async fn list_dir(args: ListDirArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let worktree: PathBuf = ctx
        .state()
        .get_state("worktree_path")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let dir_path = match &args.path {
        Some(p) if !p.is_empty() => worktree.join(p),
        _ => worktree.clone(),
    };

    // Security check
    if !dir_path.starts_with(&worktree) {
        return ToolResult::error("Path escapes worktree sandbox");
    }

    match std::fs::read_dir(&dir_path) {
        Ok(entries) => {
            let mut files = Vec::new();
            let mut dirs = Vec::new();

            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.path().is_dir() {
                    dirs.push(name);
                } else {
                    files.push(name);
                }
            }

            ToolResult::success(json!({
                "path": args.path.unwrap_or_else(|| ".".to_string()),
                "files": files,
                "directories": dirs
            }))
        }
        Err(e) => ToolResult::error(format!("Failed to list directory: {}", e)),
    }
}
