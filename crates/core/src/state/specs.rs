//! # Spec Manager
//!
//! Manages project specifications (architecture, features, etc.) in the database.
//! Replaces the filesystem-based hybrid spec system with SQL queries.

use super::db::CatalystDb;
use anyhow::Result;

/// Manager for project specifications
///
/// Reads/writes spec fragments from the `project_documents` table.
pub struct SpecManager<'a> {
    db: &'a CatalystDb,
}

impl<'a> SpecManager<'a> {
    /// Create a new spec manager with a database reference
    pub fn new(db: &'a CatalystDb) -> Self {
        Self { db }
    }

    /// Read the main spec document (spec.md equivalent)
    pub fn read_index(&self) -> Result<String> {
        self.db.get_document("spec").map(|(_, content)| content)
    }

    /// Read a spec fragment (e.g., "features", "architecture")
    pub fn read_fragment(&self, name: &str) -> Result<String> {
        let slug = normalize_slug(name);
        self.db.get_document(&slug).map(|(_, content)| content)
    }

    /// Write a spec fragment
    pub fn write_fragment(&self, name: &str, content: &str) -> Result<()> {
        let slug = normalize_slug(name);
        let title = slug_to_title(&slug);
        self.db.set_document(&slug, &title, content)
    }

    /// List all available spec fragments
    pub fn list_fragments(&self) -> Result<Vec<String>> {
        self.db.list_documents()
    }

    /// Create a spec fragment if it doesn't exist
    pub fn ensure_fragment(&self, name: &str, default_content: &str) -> Result<()> {
        let slug = normalize_slug(name);
        if self.read_fragment(&slug).is_err() {
            self.write_fragment(&slug, default_content)?;
        }
        Ok(())
    }

    /// Seed default project documents if empty
    pub fn seed_defaults(&self) -> Result<usize> {
        let existing = self.db.list_documents()?;
        if !existing.is_empty() {
            return Ok(0);
        }

        let defaults = vec![
            ("spec", "Project Specification", DEFAULT_SPEC),
            ("architecture", "Architecture", DEFAULT_ARCHITECTURE),
            ("features", "Features", DEFAULT_FEATURES),
            ("constraints", "Constraints", DEFAULT_CONSTRAINTS),
            ("unknowns", "Unknowns", DEFAULT_UNKNOWNS),
            ("decisions", "Decisions", DEFAULT_DECISIONS),
        ];

        let mut count = 0;
        for (slug, title, content) in defaults {
            self.db.set_document(slug, title, content)?;
            count += 1;
        }

        tracing::info!("Seeded {} default project documents", count);
        Ok(count)
    }
}

/// Normalize a fragment name to a slug
fn normalize_slug(name: &str) -> String {
    name.trim()
        .trim_end_matches(".md")
        .to_lowercase()
        .replace(' ', "_")
}

/// Convert a slug to a display title
fn slug_to_title(slug: &str) -> String {
    slug.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ============================================================================
// Default Document Content
// ============================================================================

const DEFAULT_SPEC: &str = r#"# Project Specification

## Overview
[Project description goes here]

## Tech Stack
| Component | Technology | Version | Notes |
|-----------|------------|---------|-------|
| Backend | Rust | 1.70+ | |
| Framework | Axum | 0.7+ | |
| Database | SQLite | 3.x | |

## See Also
- [Architecture](./architecture.md)
- [Features](./features.md)
- [Constraints](./constraints.md)
"#;

const DEFAULT_ARCHITECTURE: &str = r#"# System Architecture

## Overview
[High-level architecture description]

## Components
[Component breakdown]

## Data Flow
[How data moves through the system]
"#;

const DEFAULT_FEATURES: &str = r#"# Features

## Implemented
- [ ] Core functionality

## Planned
- [ ] Future features
"#;

const DEFAULT_CONSTRAINTS: &str = r#"# Code Constraints

## File Limits
- Maximum file length: 150 lines
- Maximum function length: 30 lines

## Error Handling
- Use `anyhow::Result` for fallible operations
- No `unwrap()` in production code

## Style
- Follow Rust conventions
- Use descriptive names
"#;

const DEFAULT_UNKNOWNS: &str = r#"# Unknowns

Technical and architectural questions that need resolution.

## Open
[No open unknowns]

## Resolved
[No resolved unknowns yet]
"#;

const DEFAULT_DECISIONS: &str = r#"# Architecture Decision Records

## ADR-001: [Title]
**Status:** Proposed | Accepted | Deprecated | Superseded

**Context:** [Why this decision is needed]

**Decision:** [What we decided]

**Consequences:** [What results from this decision]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_slug() {
        assert_eq!(normalize_slug("architecture.md"), "architecture");
        assert_eq!(normalize_slug("FEATURES"), "features");
        assert_eq!(normalize_slug("  spec.md  "), "spec");
        assert_eq!(normalize_slug("my feature"), "my_feature");
    }

    #[test]
    fn test_slug_to_title() {
        assert_eq!(slug_to_title("architecture"), "Architecture");
        assert_eq!(slug_to_title("my_feature"), "My Feature");
    }
}
