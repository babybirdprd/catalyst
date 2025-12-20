//! # Git Tools
//!
//! Tools for git worktree operations and version control.
//! Uses ToolContext to maintain project_root state across calls.

use radkit::macros::tool;
use radkit::tools::ToolResult;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

/// Arguments for creating a worktree
#[derive(Deserialize, JsonSchema)]
pub struct CreateWorktreeArgs {
    /// Feature ID to create worktree for
    pub feature_id: String,
}

/// Create a new git worktree for isolated feature development
#[tool(
    description = "Create a git worktree for isolated feature development. Returns the worktree path."
)]
pub async fn create_worktree(args: CreateWorktreeArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let project_root: PathBuf = ctx
        .state()
        .get_state("project_root")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let branch_name = format!("catalyst/{}", args.feature_id);
    let worktree_path = project_root
        .join(".catalyst")
        .join("worktrees")
        .join(&args.feature_id);

    // Create worktree directory
    if let Err(e) = std::fs::create_dir_all(&worktree_path) {
        return ToolResult::error(format!("Failed to create worktree directory: {}", e));
    }

    // Create branch from HEAD
    let branch_output = Command::new("git")
        .args(["branch", &branch_name])
        .current_dir(&project_root)
        .output();

    if let Err(e) = branch_output {
        return ToolResult::error(format!("Failed to create branch: {}", e));
    }

    // Add worktree
    let worktree_output = Command::new("git")
        .args([
            "worktree",
            "add",
            &worktree_path.to_string_lossy(),
            &branch_name,
        ])
        .current_dir(&project_root)
        .output();

    match worktree_output {
        Ok(out) if out.status.success() => {
            // Store worktree path in context for other tools to use
            ctx.state().set_state("worktree_path", json!(worktree_path));

            ToolResult::success(json!({
                "feature_id": args.feature_id,
                "branch": branch_name,
                "worktree_path": worktree_path.to_string_lossy()
            }))
        }
        Ok(out) => ToolResult::error(format!(
            "Git worktree add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )),
        Err(e) => ToolResult::error(format!("Failed to run git worktree: {}", e)),
    }
}

/// Arguments for merging a worktree
#[derive(Deserialize, JsonSchema)]
pub struct MergeWorktreeArgs {
    /// Feature ID to merge
    pub feature_id: String,
}

/// Merge a feature worktree back to main branch
#[tool(description = "Merge a feature worktree back to main. Returns success or conflict info.")]
pub async fn merge_worktree(args: MergeWorktreeArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let project_root: PathBuf = ctx
        .state()
        .get_state("project_root")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let branch_name = format!("catalyst/{}", args.feature_id);

    // Checkout main
    let checkout = Command::new("git")
        .args(["checkout", "main"])
        .current_dir(&project_root)
        .output()
        .or_else(|_| {
            Command::new("git")
                .args(["checkout", "master"])
                .current_dir(&project_root)
                .output()
        });

    if let Err(e) = checkout {
        return ToolResult::error(format!("Failed to checkout main: {}", e));
    }

    // Attempt merge
    let merge = Command::new("git")
        .args(["merge", "--no-commit", "--no-ff", &branch_name])
        .current_dir(&project_root)
        .output();

    match merge {
        Ok(out) if out.status.success() => {
            // Commit the merge
            let commit = Command::new("git")
                .args([
                    "commit",
                    "-m",
                    &format!("feat: merge {} (squash)", args.feature_id),
                ])
                .current_dir(&project_root)
                .output();

            match commit {
                Ok(_) => ToolResult::success(json!({
                    "status": "success",
                    "feature_id": args.feature_id,
                    "merged_branch": branch_name
                })),
                Err(e) => ToolResult::error(format!("Failed to commit merge: {}", e)),
            }
        }
        Ok(out) => {
            // Check for conflicts
            let status = Command::new("git")
                .args(["diff", "--name-only", "--diff-filter=U"])
                .current_dir(&project_root)
                .output();

            let conflicts: Vec<String> = status
                .map(|s| {
                    String::from_utf8_lossy(&s.stdout)
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();

            if !conflicts.is_empty() {
                ToolResult::success(json!({
                    "status": "conflicts",
                    "feature_id": args.feature_id,
                    "conflicting_files": conflicts
                }))
            } else {
                ToolResult::error(format!(
                    "Merge failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ))
            }
        }
        Err(e) => ToolResult::error(format!("Failed to run git merge: {}", e)),
    }
}

/// Arguments for deleting a worktree
#[derive(Deserialize, JsonSchema)]
pub struct DeleteWorktreeArgs {
    /// Feature ID to delete
    pub feature_id: String,
}

/// Delete a worktree and its branch
#[tool(description = "Delete a worktree and its associated branch.")]
pub async fn delete_worktree(args: DeleteWorktreeArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let project_root: PathBuf = ctx
        .state()
        .get_state("project_root")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let worktree_path = project_root
        .join(".catalyst")
        .join("worktrees")
        .join(&args.feature_id);
    let branch_name = format!("catalyst/{}", args.feature_id);

    // Remove worktree
    let _ = Command::new("git")
        .args([
            "worktree",
            "remove",
            "--force",
            &worktree_path.to_string_lossy(),
        ])
        .current_dir(&project_root)
        .output();

    // Remove directory if still exists
    if worktree_path.exists() {
        let _ = std::fs::remove_dir_all(&worktree_path);
    }

    // Delete branch
    let _ = Command::new("git")
        .args(["branch", "-D", &branch_name])
        .current_dir(&project_root)
        .output();

    ToolResult::success(json!({
        "deleted": true,
        "feature_id": args.feature_id
    }))
}
