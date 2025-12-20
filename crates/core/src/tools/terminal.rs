//! # Terminal Command Parser
//!
//! Provides structured output from cargo and npm commands.
//! Agents receive actionable data, not unparsed logs.
//!
//! ## Philosophy
//!
//! Agents must NEVER execute raw shell commands. They use these
//! provided Tools which return structured JSON.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// A compiler error with structured fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerError {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub code: Option<String>,
    pub level: ErrorLevel,
}

/// Error severity level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrorLevel {
    Error,
    Warning,
    Note,
    Help,
}

/// Result of a single test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub duration_ms: Option<u64>,
    pub message: Option<String>,
}

/// Summary of a test run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub passed: u32,
    pub failed: u32,
    pub ignored: u32,
    pub total: u32,
    pub results: Vec<TestResult>,
}

/// Allowed commands whitelist
const ALLOWED_CARGO_COMMANDS: &[&str] =
    &["check", "build", "test", "clippy", "fmt", "doc", "clean"];

const ALLOWED_NPM_COMMANDS: &[&str] = &["run build", "run test", "run lint", "install"];

const ALLOWED_GIT_COMMANDS: &[&str] = &["status", "diff", "add", "commit", "log", "show"];

/// Blocked command patterns (security)
/// These patterns are rejected even within allowed commands
const BLOCKED_PATTERNS: &[&str] = &[
    // Destructive file operations
    "rm ", "rmdir", "del ", "erase", // Network operations
    "curl", "wget", "fetch", "nc ", "netcat", // Privilege escalation
    "sudo", "su ", "doas", "runas", // Arbitrary execution
    "eval", "exec", "source", "bash -c", "sh -c", "cmd /c",
    // Dangerous redirects and pipes (when used unsafely)
    "> /", ">> /", "| rm", "| sh", "| bash", // Environment manipulation
    "export ", "unset ", "env ", // Process control
    "kill ", "pkill", "killall",
];

/// Run cargo check and return structured errors
pub async fn run_cargo_check(cwd: &Path) -> Result<Vec<CompilerError>> {
    run_cargo_command(cwd, &["check", "--message-format=json"]).await
}

/// Run cargo build and return structured errors
pub async fn run_cargo_build(cwd: &Path, release: bool) -> Result<Vec<CompilerError>> {
    let mut args = vec!["build", "--message-format=json"];
    if release {
        args.push("--release");
    }
    run_cargo_command(cwd, &args).await
}

/// Run cargo clippy and return structured warnings/errors
pub async fn run_cargo_clippy(cwd: &Path) -> Result<Vec<CompilerError>> {
    run_cargo_command(
        cwd,
        &["clippy", "--message-format=json", "--", "-D", "warnings"],
    )
    .await
}

/// Run cargo test and return structured summary
pub async fn run_cargo_test(cwd: &Path) -> Result<TestSummary> {
    let output = Command::new("cargo")
        .args(["test", "--", "--format=json", "-Z", "unstable-options"])
        .current_dir(cwd)
        .output()
        .context("Failed to run cargo test")?;

    // Parse the JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_test_output(&stdout)
}

/// Run cargo fmt check (returns true if formatted correctly)
pub async fn run_cargo_fmt_check(cwd: &Path) -> Result<bool> {
    let output = Command::new("cargo")
        .args(["fmt", "--check"])
        .current_dir(cwd)
        .output()
        .context("Failed to run cargo fmt")?;

    Ok(output.status.success())
}

/// Validate that a command is allowed
pub fn validate_command(command: &str, args: &[&str]) -> Result<()> {
    // First, check for blocked patterns in the full command
    let full_command = format!("{} {}", command, args.join(" "));
    for blocked in BLOCKED_PATTERNS {
        if full_command.contains(blocked) {
            return Err(anyhow::anyhow!(
                "Command contains blocked pattern: '{}'. This operation is not permitted.",
                blocked.trim()
            ));
        }
    }

    // Then check against allowed command whitelist
    match command {
        "cargo" => {
            if let Some(subcommand) = args.first() {
                if !ALLOWED_CARGO_COMMANDS.contains(subcommand) {
                    return Err(anyhow::anyhow!(
                        "Cargo subcommand '{}' is not allowed. Allowed: {:?}",
                        subcommand,
                        ALLOWED_CARGO_COMMANDS
                    ));
                }
            }
        }
        "npm" | "pnpm" => {
            let full_cmd = args.join(" ");
            if !ALLOWED_NPM_COMMANDS.iter().any(|a| full_cmd.starts_with(a)) {
                return Err(anyhow::anyhow!(
                    "npm/pnpm command '{}' is not allowed. Allowed: {:?}",
                    full_cmd,
                    ALLOWED_NPM_COMMANDS
                ));
            }
        }
        "git" => {
            if let Some(subcommand) = args.first() {
                if !ALLOWED_GIT_COMMANDS.contains(subcommand) {
                    return Err(anyhow::anyhow!(
                        "Git subcommand '{}' is not allowed. Allowed: {:?}",
                        subcommand,
                        ALLOWED_GIT_COMMANDS
                    ));
                }
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Command '{}' is not in the allowed list",
                command
            ));
        }
    }
    Ok(())
}

// --- Private helpers ---

async fn run_cargo_command(cwd: &Path, args: &[&str]) -> Result<Vec<CompilerError>> {
    let output = Command::new("cargo")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("Failed to run cargo {:?}", args))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_cargo_output(&stdout)
}

fn parse_cargo_output(output: &str) -> Result<Vec<CompilerError>> {
    let mut errors = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON line
        if let Ok(msg) = serde_json::from_str::<CargoMessage>(line) {
            if let Some(message) = msg.message {
                if let Some(spans) = message.spans.first() {
                    errors.push(CompilerError {
                        file: spans.file_name.clone(),
                        line: spans.line_start,
                        column: spans.column_start,
                        message: message.message,
                        code: message.code.map(|c| c.code),
                        level: match message.level.as_str() {
                            "error" => ErrorLevel::Error,
                            "warning" => ErrorLevel::Warning,
                            "note" => ErrorLevel::Note,
                            _ => ErrorLevel::Help,
                        },
                    });
                }
            }
        }
    }

    Ok(errors)
}

fn parse_test_output(output: &str) -> Result<TestSummary> {
    let mut results = Vec::new();
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut ignored = 0u32;

    for line in output.lines() {
        if let Ok(event) = serde_json::from_str::<TestEvent>(line) {
            match event.event.as_str() {
                "ok" => {
                    passed += 1;
                    results.push(TestResult {
                        name: event.name.unwrap_or_default(),
                        passed: true,
                        duration_ms: None,
                        message: None,
                    });
                }
                "failed" => {
                    failed += 1;
                    results.push(TestResult {
                        name: event.name.unwrap_or_default(),
                        passed: false,
                        duration_ms: None,
                        message: event.stdout,
                    });
                }
                "ignored" => {
                    ignored += 1;
                }
                _ => {}
            }
        }
    }

    Ok(TestSummary {
        passed,
        failed,
        ignored,
        total: passed + failed + ignored,
        results,
    })
}

// --- Cargo JSON message types ---

#[derive(Debug, Deserialize)]
struct CargoMessage {
    message: Option<DiagnosticMessage>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticMessage {
    message: String,
    level: String,
    code: Option<DiagnosticCode>,
    spans: Vec<DiagnosticSpan>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticCode {
    code: String,
}

#[derive(Debug, Deserialize)]
struct DiagnosticSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
}

#[derive(Debug, Deserialize)]
struct TestEvent {
    event: String,
    name: Option<String>,
    stdout: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_allowed_cargo() {
        assert!(validate_command("cargo", &["check"]).is_ok());
        assert!(validate_command("cargo", &["build"]).is_ok());
        assert!(validate_command("cargo", &["rm"]).is_err());
    }

    #[test]
    fn test_validate_disallowed_command() {
        assert!(validate_command("rm", &["-rf", "/"]).is_err());
        assert!(validate_command("curl", &["evil.com"]).is_err());
    }
}
