//! # 3-Truth Synthesis Merge Conflict Resolution
//!
//! Resolves merge conflicts by extracting clean file versions using git plumbing,
//! rather than parsing conflict markers which are error-prone.
//!
//! ## The Protocol
//!
//! 1. **Extract**: Use `git show :1/:2/:3:path` to get base/ours/theirs
//! 2. **Synthesize**: Agent compares and combines changes
//! 3. **Verify**: Run appropriate linter (cargo check, pnpm lint)
//! 4. **Apply**: Overwrite and stage the file

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Represents the three versions of a conflicting file
#[derive(Debug, Clone)]
pub struct ConflictFile {
    /// Path to the conflicting file (relative to repo root)
    pub path: PathBuf,
    /// Common ancestor version (git show :1:path)
    pub base: String,
    /// Current branch version (git show :2:path) - "ours"
    pub ours: String,
    /// Incoming branch version (git show :3:path) - "theirs"
    pub theirs: String,
}

/// Extract the three clean versions of a conflicting file
///
/// Uses git plumbing commands to get versions without parsing markers.
pub fn extract_conflict_versions(repo_path: &Path, file_path: &Path) -> Result<ConflictFile> {
    let file_str = file_path.to_string_lossy();

    // Extract base (stage 1)
    let base = git_show(repo_path, &format!(":1:{}", file_str)).unwrap_or_default();

    // Extract ours (stage 2)
    let ours = git_show(repo_path, &format!(":2:{}", file_str))
        .with_context(|| format!("Failed to extract 'ours' version of {}", file_str))?;

    // Extract theirs (stage 3)
    let theirs = git_show(repo_path, &format!(":3:{}", file_str))
        .with_context(|| format!("Failed to extract 'theirs' version of {}", file_str))?;

    Ok(ConflictFile {
        path: file_path.to_path_buf(),
        base,
        ours,
        theirs,
    })
}

/// Get list of files with merge conflicts
pub fn get_conflicting_files(repo_path: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git diff")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();

    Ok(files)
}

/// Verify a resolved file using the appropriate tool
pub fn verify_resolution(repo_path: &Path, file_path: &Path) -> Result<VerifyResult> {
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => verify_rust(repo_path),
        "ts" | "tsx" | "js" | "jsx" => verify_typescript(repo_path),
        "json" => verify_json(repo_path, file_path),
        _ => Ok(VerifyResult::Passed),
    }
}

/// Result of verification
#[derive(Debug)]
pub enum VerifyResult {
    Passed,
    Failed { errors: Vec<String> },
}

/// Apply a resolved file and stage it
pub fn apply_resolution(repo_path: &Path, file_path: &Path, content: &str) -> Result<()> {
    let full_path = repo_path.join(file_path);

    // Write the resolved content
    std::fs::write(&full_path, content)
        .with_context(|| format!("Failed to write resolved file: {:?}", full_path))?;

    // Stage the file
    Command::new("git")
        .args(["add", &file_path.to_string_lossy()])
        .current_dir(repo_path)
        .output()
        .context("Failed to stage resolved file")?;

    Ok(())
}

/// Handle lockfile conflicts by regenerating
pub fn regenerate_lockfile(repo_path: &Path, lockfile: &Path) -> Result<()> {
    let filename = lockfile.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Remove the conflicting lockfile
    let full_path = repo_path.join(lockfile);
    if full_path.exists() {
        std::fs::remove_file(&full_path)?;
    }

    // Regenerate based on type
    match filename {
        "Cargo.lock" => {
            Command::new("cargo")
                .args(["generate-lockfile"])
                .current_dir(repo_path)
                .output()
                .context("Failed to regenerate Cargo.lock")?;
        }
        "pnpm-lock.yaml" => {
            Command::new("pnpm")
                .args(["install", "--lockfile-only"])
                .current_dir(repo_path)
                .output()
                .context("Failed to regenerate pnpm-lock.yaml")?;
        }
        "package-lock.json" => {
            Command::new("npm")
                .args(["install", "--package-lock-only"])
                .current_dir(repo_path)
                .output()
                .context("Failed to regenerate package-lock.json")?;
        }
        _ => {}
    }

    Ok(())
}

// --- Private helpers ---

fn git_show(repo_path: &Path, spec: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["show", spec])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git show")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git show {} failed: {}",
            spec,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn verify_rust(repo_path: &Path) -> Result<VerifyResult> {
    let output = Command::new("cargo")
        .args(["check", "--message-format=short"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run cargo check")?;

    if output.status.success() {
        Ok(VerifyResult::Passed)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let errors: Vec<String> = stderr.lines().map(String::from).collect();
        Ok(VerifyResult::Failed { errors })
    }
}

fn verify_typescript(repo_path: &Path) -> Result<VerifyResult> {
    // Try pnpm first, then npm
    let output = Command::new("pnpm")
        .args(["lint"])
        .current_dir(repo_path)
        .output()
        .or_else(|_| {
            Command::new("npm")
                .args(["run", "lint"])
                .current_dir(repo_path)
                .output()
        })
        .context("Failed to run linter")?;

    if output.status.success() {
        Ok(VerifyResult::Passed)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let errors: Vec<String> = stderr.lines().map(String::from).collect();
        Ok(VerifyResult::Failed { errors })
    }
}

fn verify_json(repo_path: &Path, file_path: &Path) -> Result<VerifyResult> {
    let full_path = repo_path.join(file_path);
    let content = std::fs::read_to_string(&full_path)?;

    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(_) => Ok(VerifyResult::Passed),
        Err(e) => Ok(VerifyResult::Failed {
            errors: vec![e.to_string()],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_result_enum() {
        let passed = VerifyResult::Passed;
        assert!(matches!(passed, VerifyResult::Passed));

        let failed = VerifyResult::Failed {
            errors: vec!["test error".to_string()],
        };
        assert!(matches!(failed, VerifyResult::Failed { .. }));
    }
}
