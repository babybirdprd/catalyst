//! # Scanner - Semantic Code Indexer
//!
//! Builds a symbol index from Rust files using `syn`.
//! For TypeScript, falls back to regex patterns.
//!
//! ## Unified API
//!
//! Use `analyze_codebase()` for comprehensive analysis combining:
//! - `syn`-based symbol extraction (structs, traits, functions)
//! - Tree-sitter semantic map (module hierarchy, dependencies)

use crate::tools::ast_scanner::{self, SemanticMap};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A symbol extracted from source code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Full qualified path (e.g., "module::StructName::method")
    pub path: String,
    /// Type of symbol
    pub kind: SymbolKind,
    /// File where defined
    pub file: PathBuf,
    /// Line number
    pub line: u32,
    /// Signature if applicable
    #[serde(default)]
    pub signature: Option<String>,
}

/// Type of symbol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Struct,
    Enum,
    Function,
    Trait,
    Impl,
    Const,
    Type,
    Module,
    Export, // For TypeScript
}

/// The symbol index
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolIndex {
    pub symbols: Vec<SymbolInfo>,
    #[serde(default)]
    pub by_name: HashMap<String, Vec<usize>>,
}

impl SymbolIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a symbol to the index
    pub fn add(&mut self, symbol: SymbolInfo) {
        let name = symbol.name.clone();
        let idx = self.symbols.len();
        self.symbols.push(symbol);
        self.by_name.entry(name).or_default().push(idx);
    }

    /// Find symbols by name
    pub fn find_by_name(&self, name: &str) -> Vec<&SymbolInfo> {
        self.by_name
            .get(name)
            .map(|indices| indices.iter().map(|&i| &self.symbols[i]).collect())
            .unwrap_or_default()
    }

    /// Search symbols by pattern
    pub fn search(&self, pattern: &str) -> Vec<&SymbolInfo> {
        let pattern_lower = pattern.to_lowercase();
        self.symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&pattern_lower))
            .collect()
    }
}

/// Build an index from a directory
pub fn build_index(dir: &Path) -> Result<SymbolIndex> {
    let mut index = SymbolIndex::new();

    let walker = ignore::WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                match ext {
                    "rs" => {
                        if let Ok(symbols) = index_rust_file(path) {
                            for s in symbols {
                                index.add(s);
                            }
                        }
                    }
                    "ts" | "tsx" => {
                        if let Ok(symbols) = index_typescript_file(path) {
                            for s in symbols {
                                index.add(s);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(index)
}

// ============================================================================
// Unified Codebase Analysis
// ============================================================================

/// Combined result of syn-based and tree-sitter analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseAnalysis {
    /// Flat symbol index (syn-based)
    pub symbols: SymbolIndex,
    /// Hierarchical semantic map (tree-sitter based)
    pub semantic: SemanticMap,
}

/// Analyze a codebase using both syn and tree-sitter
///
/// Returns a unified view combining:
/// - Symbol lookup from syn parsing
/// - Module hierarchy and dependencies from tree-sitter
pub async fn analyze_codebase(dir: &Path) -> Result<CodebaseAnalysis> {
    let dir_clone = dir.to_path_buf();

    // Run syn-based indexing (blocking)
    let symbols = tokio::task::spawn_blocking({
        let dir = dir_clone.clone();
        move || build_index(&dir)
    })
    .await??;

    // Run tree-sitter analysis
    let semantic = ast_scanner::build_semantic_map(&dir_clone).await?;

    Ok(CodebaseAnalysis { symbols, semantic })
}

/// Signature info for brownfield profile
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub file: std::path::PathBuf,
    pub line: u32,
}

/// Extract public API signatures from Rust project (async wrapper)
pub async fn extract_rust_signatures(root: &Path) -> Result<Vec<SignatureInfo>> {
    let root = root.to_path_buf();

    // Run sync code in blocking task
    tokio::task::spawn_blocking(move || extract_rust_signatures_sync(&root)).await?
}

/// Extract public API signatures (sync implementation)
fn extract_rust_signatures_sync(root: &Path) -> Result<Vec<SignatureInfo>> {
    let index = build_index(root)?;

    let signatures: Vec<SignatureInfo> = index
        .symbols
        .into_iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Struct | SymbolKind::Trait | SymbolKind::Function | SymbolKind::Enum
            )
        })
        .map(|s| SignatureInfo {
            name: s.name,
            kind: format!("{:?}", s.kind).to_lowercase(),
            signature: s.signature.unwrap_or_default(),
            file: s.file,
            line: s.line,
        })
        .collect();

    Ok(signatures)
}

/// Index a Rust file using syn
fn index_rust_file(path: &Path) -> Result<Vec<SymbolInfo>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;

    let file = syn::parse_file(&content).with_context(|| format!("Failed to parse {:?}", path))?;

    let mut symbols = Vec::new();

    for item in &file.items {
        if let Some(symbol) = extract_item_symbol(item, path) {
            symbols.push(symbol);
        }
    }

    Ok(symbols)
}

/// Extract symbol info from a syn Item
fn extract_item_symbol(item: &syn::Item, path: &Path) -> Option<SymbolInfo> {
    match item {
        syn::Item::Struct(s) => Some(SymbolInfo {
            name: s.ident.to_string(),
            path: s.ident.to_string(),
            kind: SymbolKind::Struct,
            file: path.to_path_buf(),
            line: 0, // syn doesn't easily give line numbers
            signature: None,
        }),
        syn::Item::Enum(e) => Some(SymbolInfo {
            name: e.ident.to_string(),
            path: e.ident.to_string(),
            kind: SymbolKind::Enum,
            file: path.to_path_buf(),
            line: 0,
            signature: None,
        }),
        syn::Item::Fn(f) => Some(SymbolInfo {
            name: f.sig.ident.to_string(),
            path: f.sig.ident.to_string(),
            kind: SymbolKind::Function,
            file: path.to_path_buf(),
            line: 0,
            signature: Some(format!("fn {}(...)", f.sig.ident)),
        }),
        syn::Item::Trait(t) => Some(SymbolInfo {
            name: t.ident.to_string(),
            path: t.ident.to_string(),
            kind: SymbolKind::Trait,
            file: path.to_path_buf(),
            line: 0,
            signature: None,
        }),
        _ => None,
    }
}

/// Index a TypeScript file using regex patterns (fallback)
fn index_typescript_file(path: &Path) -> Result<Vec<SymbolInfo>> {
    let content = std::fs::read_to_string(path)?;
    let mut symbols = Vec::new();

    // Simple regex patterns for common TypeScript constructs
    let patterns = [
        (r"export\s+(?:interface|type)\s+(\w+)", SymbolKind::Type),
        (r"export\s+function\s+(\w+)", SymbolKind::Function),
        (r"export\s+class\s+(\w+)", SymbolKind::Struct),
        (r"export\s+const\s+(\w+)", SymbolKind::Const),
        (r"export\s+\{([^}]+)\}", SymbolKind::Export),
    ];

    for (pattern, kind) in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            for (line_num, line) in content.lines().enumerate() {
                for cap in re.captures_iter(line) {
                    if let Some(m) = cap.get(1) {
                        // Handle multiple exports in braces
                        if *kind == SymbolKind::Export {
                            for name in m.as_str().split(',') {
                                let name = name.trim();
                                if !name.is_empty() {
                                    symbols.push(SymbolInfo {
                                        name: name.to_string(),
                                        path: name.to_string(),
                                        kind: SymbolKind::Export,
                                        file: path.to_path_buf(),
                                        line: (line_num + 1) as u32,
                                        signature: None,
                                    });
                                }
                            }
                        } else {
                            symbols.push(SymbolInfo {
                                name: m.as_str().to_string(),
                                path: m.as_str().to_string(),
                                kind: kind.clone(),
                                file: path.to_path_buf(),
                                line: (line_num + 1) as u32,
                                signature: None,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_index() {
        let mut index = SymbolIndex::new();
        index.add(SymbolInfo {
            name: "TestStruct".to_string(),
            path: "TestStruct".to_string(),
            kind: SymbolKind::Struct,
            file: PathBuf::from("test.rs"),
            line: 1,
            signature: None,
        });

        let results = index.find_by_name("TestStruct");
        assert_eq!(results.len(), 1);
    }
}
