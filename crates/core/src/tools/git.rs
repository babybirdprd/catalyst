//! # Git Worktree Isolation
//!
//! Provides sandboxed git worktrees for feature development.
//! Each feature gets its own isolated branch and working directory.

use anyhow::{Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::state::io::get_runtime_path;

/// Create a new worktree for a feature
///
/// Creates a new branch `catalyst/<feature_id>` and checks it out
/// in `.catalyst/worktrees/<feature_id>`.
pub fn create_worktree(project_root: &Path, feature_id: &str) -> Result<PathBuf> {
    let repo = Repository::open(project_root)
        .with_context(|| format!("Failed to open repository at {:?}", project_root))?;

    let branch_name = format!("catalyst/{}", feature_id);
    let worktree_path = get_runtime_path().join("worktrees").join(feature_id);

    // Ensure worktree directory exists
    std::fs::create_dir_all(&worktree_path)
        .with_context(|| format!("Failed to create worktree directory: {:?}", worktree_path))?;

    // Get HEAD commit to branch from
    let head = repo.head().context("Failed to get HEAD")?;
    let head_commit = head.peel_to_commit().context("Failed to get HEAD commit")?;

    // Create the branch
    repo.branch(&branch_name, &head_commit, false)
        .with_context(|| format!("Failed to create branch: {}", branch_name))?;

    // Add the worktree using git CLI (more reliable for worktree operations)
    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            &worktree_path.to_string_lossy(),
            &branch_name,
        ])
        .current_dir(project_root)
        .output()
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(worktree_path)
}

/// Merge a feature worktree back to main using squash merge
///
/// Squashes all commits from the feature branch into a single commit on main.
pub fn merge_worktree(project_root: &Path, feature_id: &str) -> Result<MergeResult> {
    let branch_name = format!("catalyst/{}", feature_id);

    // Checkout main
    let checkout = Command::new("git")
        .args(["checkout", "main"])
        .current_dir(project_root)
        .output()
        .or_else(|_| {
            Command::new("git")
                .args(["checkout", "master"])
                .current_dir(project_root)
                .output()
        })
        .context("Failed to checkout main/master")?;

    if !checkout.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to checkout main: {}",
            String::from_utf8_lossy(&checkout.stderr)
        ));
    }

    // Attempt merge with --no-commit to check for conflicts first
    let merge = Command::new("git")
        .args(["merge", "--no-commit", "--no-ff", &branch_name])
        .current_dir(project_root)
        .output()
        .context("Failed to run git merge")?;

    if !merge.status.success() {
        // Check if there are conflicts
        let status = Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=U"])
            .current_dir(project_root)
            .output()?;

        let conflicts: Vec<String> = String::from_utf8_lossy(&status.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();

        if !conflicts.is_empty() {
            return Ok(MergeResult::Conflicts(conflicts));
        }

        return Err(anyhow::anyhow!(
            "Merge failed: {}",
            String::from_utf8_lossy(&merge.stderr)
        ));
    }

    // Commit the merge
    let commit = Command::new("git")
        .args([
            "commit",
            "-m",
            &format!("feat: merge {} (squash)", feature_id),
        ])
        .current_dir(project_root)
        .output()
        .context("Failed to commit merge")?;

    if !commit.status.success() {
        // No changes to commit is ok
        let stderr = String::from_utf8_lossy(&commit.stderr);
        if !stderr.contains("nothing to commit") {
            return Err(anyhow::anyhow!("Failed to commit: {}", stderr));
        }
    }

    Ok(MergeResult::Success)
}

/// Result of a merge operation
#[derive(Debug)]
pub enum MergeResult {
    /// Merge completed successfully
    Success,
    /// Merge has conflicts that need resolution
    Conflicts(Vec<String>),
}

/// Delete a worktree and its associated branch
pub fn delete_worktree(project_root: &Path, feature_id: &str) -> Result<()> {
    let worktree_path = get_runtime_path().join("worktrees").join(feature_id);
    let branch_name = format!("catalyst/{}", feature_id);

    // Remove the worktree using git CLI
    Command::new("git")
        .args([
            "worktree",
            "remove",
            "--force",
            &worktree_path.to_string_lossy(),
        ])
        .current_dir(project_root)
        .output()
        .ok(); // Ignore errors if worktree doesn't exist

    // Remove the worktree directory if it still exists
    if worktree_path.exists() {
        std::fs::remove_dir_all(&worktree_path)
            .with_context(|| format!("Failed to remove worktree directory: {:?}", worktree_path))?;
    }

    // Delete the branch
    Command::new("git")
        .args(["branch", "-D", &branch_name])
        .current_dir(project_root)
        .output()
        .ok(); // Ignore errors if branch doesn't exist

    Ok(())
}

/// List all active feature worktrees
pub fn list_worktrees(project_root: &Path) -> Result<Vec<String>> {
    let repo = Repository::open(project_root)
        .with_context(|| format!("Failed to open repository at {:?}", project_root))?;

    let worktrees = repo.worktrees()?;
    let feature_worktrees: Vec<String> = worktrees
        .iter()
        .filter_map(|name| name)
        .filter(|name| name.starts_with("feature-"))
        .map(|name| name.strip_prefix("feature-").unwrap_or(name).to_string())
        .collect();

    Ok(feature_worktrees)
}

/// Get the worktree path for a feature
pub fn get_worktree_path(feature_id: &str) -> PathBuf {
    get_runtime_path().join("worktrees").join(feature_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_path_generation() {
        let path = get_worktree_path("test-feature");
        assert!(path.to_string_lossy().contains(".catalyst"));
        assert!(path.to_string_lossy().contains("worktrees"));
    }
}
