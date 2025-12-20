//! # Cyborg Tools
//!
//! Deterministic tools that wrap agent guesswork with structured outputs.
//!
//! ## Philosophy: "Code > Agents"
//!
//! These tools provide the "Cyborg" machinery - Rust code that replaces
//! or wraps agent capabilities with compile-time guarantees.
//!
//! ## Modules
//!
//! - `git` - Worktree isolation and merge operations
//! - `merge` - 3-Truth Synthesis conflict resolution
//! - `terminal` - Cargo command parser with structured output
//! - `linter` - Lines of code scanner (Rule of 100)
//! - `scanner` - Semantic code indexer (syn for Rust, regex for TS)
//! - `ast_scanner` - Tree-sitter based AST analysis for module graphs
//! - `search` - Symbol query tool

pub mod ast_scanner;
pub mod git;
pub mod linter;
pub mod merge;
pub mod scanner;
pub mod search;
pub mod terminal;
