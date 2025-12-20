//! # Linter - Lines of Code Scanner
//!
//! Enforces the "Rule of 100" - agent-sized code that fits in context windows.
//!
//! ## Features
//!
//! - Configurable limits for file and function LOC
//! - AST-based function detection using `syn` (with heuristic fallback)
//! - Exemption patterns for tests and generated code
//! - Structured output for A2A skill integration

use anyhow::Result;
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for constraint checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintConfig {
    /// Max lines per file (default: 150)
    pub max_file_lines: usize,
    /// Max lines per function (default: 30)
    pub max_function_lines: usize,
    /// File patterns exempt from checks (glob syntax)
    pub exemptions: Vec<String>,
}

impl Default for ConstraintConfig {
    fn default() -> Self {
        Self {
            max_file_lines: 150,
            max_function_lines: 30,
            exemptions: vec![
                "**/tests/**".into(),
                "**/test/**".into(),
                "**/*_test.rs".into(),
                "**/*_tests.rs".into(),
                "**/generated/**".into(),
            ],
        }
    }
}

impl ConstraintConfig {
    /// Check if a file path is exempt from constraints
    pub fn is_exempt(&self, path: &str) -> bool {
        for pattern in &self.exemptions {
            if let Ok(pat) = Pattern::new(pattern) {
                if pat.matches(path) {
                    return true;
                }
            }
        }
        false
    }
}

// ============================================================================
// Violation Types
// ============================================================================

/// A violation of code constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub file: String,
    pub line: Option<u32>,
    pub kind: ViolationKind,
    pub message: String,
}

/// Type of violation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    FileTooLong,
    FunctionTooLong,
    UnwrapUsed,
    MissingDocumentation,
}

// ============================================================================
// Scanning Functions
// ============================================================================

/// Scan a file for violations using default config
pub fn scan_file(path: &Path) -> Result<Vec<Violation>> {
    scan_file_with_config(path, &ConstraintConfig::default())
}

/// Scan a file for violations with custom config
pub fn scan_file_with_config(path: &Path, config: &ConstraintConfig) -> Result<Vec<Violation>> {
    let file_str = path.to_string_lossy().to_string();

    // Check exemptions
    if config.is_exempt(&file_str) {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    scan_content(&content, &file_str, config)
}

/// Scan code content directly (for testing or in-memory use)
pub fn scan_content(
    content: &str,
    file_name: &str,
    config: &ConstraintConfig,
) -> Result<Vec<Violation>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut violations = Vec::new();

    // Check file length
    if lines.len() > config.max_file_lines {
        violations.push(Violation {
            file: file_name.to_string(),
            line: None,
            kind: ViolationKind::FileTooLong,
            message: format!(
                "File has {} lines, max is {}",
                lines.len(),
                config.max_file_lines
            ),
        });
    }

    // Check for unwrap usage (simple pattern match)
    for (i, line) in lines.iter().enumerate() {
        if line.contains(".unwrap()") && !line.trim().starts_with("//") {
            violations.push(Violation {
                file: file_name.to_string(),
                line: Some((i + 1) as u32),
                kind: ViolationKind::UnwrapUsed,
                message: "Use of .unwrap() - prefer .context()? or handle error".to_string(),
            });
        }
    }

    // Use heuristic-based function scanning
    // Note: AST + quote doesn't work well for line counting (quote minifies output)
    // The heuristic is more accurate for actual source line counts
    violations.extend(scan_function_lengths_heuristic(file_name, &lines, config));

    Ok(violations)
}

/// Scan for function length violations using syn AST
fn scan_functions_ast(
    content: &str,
    file_name: &str,
    config: &ConstraintConfig,
) -> Option<Vec<Violation>> {
    use syn::{parse_file, ImplItem, Item};

    let ast = parse_file(content).ok()?;
    let mut violations = Vec::new();

    for item in ast.items {
        match item {
            Item::Fn(func) => {
                check_function_length(
                    &func.sig.ident.to_string(),
                    &quote::quote!(#func).to_string(),
                    file_name,
                    config,
                    &mut violations,
                );
            }
            Item::Impl(impl_block) => {
                for impl_item in impl_block.items {
                    if let ImplItem::Fn(method) = impl_item {
                        check_function_length(
                            &method.sig.ident.to_string(),
                            &quote::quote!(#method).to_string(),
                            file_name,
                            config,
                            &mut violations,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    Some(violations)
}

/// Check a single function's length
fn check_function_length(
    name: &str,
    func_str: &str,
    file_name: &str,
    config: &ConstraintConfig,
    violations: &mut Vec<Violation>,
) {
    let func_lines = func_str.lines().count();
    if func_lines > config.max_function_lines {
        violations.push(Violation {
            file: file_name.to_string(),
            line: None, // AST doesn't give us reliable line numbers after quote
            kind: ViolationKind::FunctionTooLong,
            message: format!(
                "Function '{}' has {} lines, max is {}",
                name, func_lines, config.max_function_lines
            ),
        });
    }
}

/// Fallback: Scan for function length violations using heuristics
fn scan_function_lengths_heuristic(
    file: &str,
    lines: &[&str],
    config: &ConstraintConfig,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let mut in_function = false;
    let mut function_start = 0;
    let mut function_name = String::new();
    let mut brace_depth = 0;

    for (i, line) in lines.iter().enumerate() {
        // Simple detection: "fn name("
        if line.contains("fn ") && line.contains("(") && !line.trim().starts_with("//") {
            if let Some(start) = line.find("fn ") {
                let after_fn = &line[start + 3..];
                if let Some(paren) = after_fn.find('(') {
                    function_name = after_fn[..paren].trim().to_string();
                    function_start = i + 1;
                    in_function = true;
                    brace_depth = 0;
                }
            }
        }

        if in_function {
            brace_depth += line.matches('{').count();
            brace_depth = brace_depth.saturating_sub(line.matches('}').count());

            if brace_depth == 0 && line.contains('}') {
                let function_len = i + 1 - function_start;
                if function_len > config.max_function_lines {
                    violations.push(Violation {
                        file: file.to_string(),
                        line: Some(function_start as u32),
                        kind: ViolationKind::FunctionTooLong,
                        message: format!(
                            "Function '{}' has {} lines, max is {}",
                            function_name, function_len, config.max_function_lines
                        ),
                    });
                }
                in_function = false;
            }
        }
    }

    violations
}

/// Scan a directory for violations
pub fn scan_directory(dir: &Path, extensions: &[&str]) -> Result<Vec<Violation>> {
    scan_directory_with_config(dir, extensions, &ConstraintConfig::default())
}

/// Scan a directory for violations with custom config
pub fn scan_directory_with_config(
    dir: &Path,
    extensions: &[&str],
    config: &ConstraintConfig,
) -> Result<Vec<Violation>> {
    let mut all_violations = Vec::new();

    let walker = ignore::WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if extensions.contains(&ext.to_str().unwrap_or("")) {
                    if let Ok(violations) = scan_file_with_config(path, config) {
                        all_violations.extend(violations);
                    }
                }
            }
        }
    }

    Ok(all_violations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_violation_kinds() {
        let v = Violation {
            file: "test.rs".to_string(),
            line: Some(10),
            kind: ViolationKind::UnwrapUsed,
            message: "test".to_string(),
        };
        assert_eq!(v.kind, ViolationKind::UnwrapUsed);
    }

    #[test]
    fn test_default_config() {
        let config = ConstraintConfig::default();
        assert_eq!(config.max_file_lines, 150);
        assert_eq!(config.max_function_lines, 30);
        assert!(!config.exemptions.is_empty());
    }

    #[test]
    fn test_exemption_patterns() {
        let config = ConstraintConfig::default();
        assert!(config.is_exempt("src/tests/foo.rs"));
        assert!(config.is_exempt("crates/core/tests/integration.rs"));
        assert!(!config.is_exempt("src/lib.rs"));
    }

    #[test]
    fn test_scan_content_file_too_long() {
        let config = ConstraintConfig {
            max_file_lines: 5,
            max_function_lines: 30,
            exemptions: vec![],
        };

        let content = "line1\nline2\nline3\nline4\nline5\nline6\nline7";
        let violations = scan_content(content, "test.rs", &config).unwrap();

        assert!(violations
            .iter()
            .any(|v| v.kind == ViolationKind::FileTooLong));
    }

    #[test]
    fn test_scan_content_function_too_long() {
        let config = ConstraintConfig {
            max_file_lines: 1000,
            max_function_lines: 3,
            exemptions: vec![],
        };

        // This function has many statements - quote will produce longer output
        let content = r#"
fn too_long() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let g = 7;
    let h = 8;
    let i = 9;
    let j = 10;
}
"#;
        let violations = scan_content(content, "test.rs", &config).unwrap();

        // Should detect function too long (either via AST or heuristic fallback)
        assert!(violations
            .iter()
            .any(|v| { v.kind == ViolationKind::FunctionTooLong }));
    }

    #[test]
    fn test_violation_serialization() {
        let v = Violation {
            file: "test.rs".to_string(),
            line: Some(10),
            kind: ViolationKind::FunctionTooLong,
            message: "test".to_string(),
        };

        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("function_too_long"));
    }
}
