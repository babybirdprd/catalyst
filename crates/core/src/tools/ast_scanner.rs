//! # AST Scanner - Tree-Sitter Based Semantic Analysis
//!
//! Provides multi-language AST analysis using tree-sitter.
//! Builds semantic maps with module relationships and dependency graphs.
//!
//! ## Design
//!
//! - Complements `syn`-based scanner which handles Rust symbol extraction
//! - Adds module hierarchy and import relationship analysis
//! - Supports TypeScript parsing for mixed codebases

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ============================================================================
// Semantic Map Types
// ============================================================================

/// A struct definition with fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDef {
    pub name: String,
    pub file: PathBuf,
    pub line: u32,
    pub is_public: bool,
    pub fields: Vec<FieldDef>,
    pub derives: Vec<String>,
}

/// A struct field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub type_name: String,
    pub is_public: bool,
}

/// A trait definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitDef {
    pub name: String,
    pub file: PathBuf,
    pub line: u32,
    pub is_public: bool,
    pub methods: Vec<String>,
}

/// A function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub file: PathBuf,
    pub line: u32,
    pub is_public: bool,
    pub is_async: bool,
    pub parameters: Vec<String>,
    pub return_type: Option<String>,
}

/// A module in the hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    pub name: String,
    pub file: PathBuf,
    pub is_public: bool,
    pub children: Vec<String>,
    pub exports: Vec<String>,
}

/// Semantic map of a codebase - hierarchical view of symbols and relationships
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticMap {
    /// All detected structs with fields
    pub structs: Vec<StructDef>,
    /// All detected traits with methods
    pub traits: Vec<TraitDef>,
    /// All standalone functions
    pub functions: Vec<FunctionDef>,
    /// Module hierarchy (path -> node)
    pub modules: HashMap<String, ModuleNode>,
    /// Import relationships (from_module, to_module)
    pub dependencies: Vec<(String, String)>,
    /// Detected impl blocks (struct_name -> trait_name or inherent)
    pub implementations: Vec<ImplInfo>,
}

/// Implementation block info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplInfo {
    /// Type being implemented
    pub target_type: String,
    /// Trait being implemented, None for inherent impl
    pub trait_name: Option<String>,
    /// Methods in this impl
    pub methods: Vec<String>,
    pub file: PathBuf,
    pub line: u32,
}

/// Module dependency graph for visualization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleGraph {
    /// Module nodes (name -> display info)
    pub nodes: HashMap<String, ModuleGraphNode>,
    /// Edges (from -> to)
    pub edges: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleGraphNode {
    pub name: String,
    pub file_count: u32,
    pub is_crate_root: bool,
}

// ============================================================================
// AST Parsing with Tree-Sitter
// ============================================================================

/// Build semantic map from a directory using tree-sitter
pub async fn build_semantic_map(root: &Path) -> Result<SemanticMap> {
    let root = root.to_path_buf();

    // Run blocking tree-sitter parsing in spawn_blocking
    tokio::task::spawn_blocking(move || build_semantic_map_sync(&root)).await?
}

/// Synchronous implementation of semantic map building
fn build_semantic_map_sync(root: &Path) -> Result<SemanticMap> {
    let mut map = SemanticMap::default();

    // Initialize tree-sitter parser for Rust
    let mut parser = tree_sitter::Parser::new();
    let rust_lang = tree_sitter_rust::LANGUAGE;
    parser
        .set_language(&rust_lang.into())
        .context("Failed to set Rust language for tree-sitter")?;

    // Walk directory for Rust files
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "rs" {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        parse_rust_file(&mut parser, &content, path, &mut map)?;
                    }
                }
            }
        }
    }

    // Build module hierarchy from collected data
    build_module_hierarchy(&mut map, root);

    Ok(map)
}

/// Parse a single Rust file and add to semantic map
fn parse_rust_file(
    parser: &mut tree_sitter::Parser,
    content: &str,
    path: &Path,
    map: &mut SemanticMap,
) -> Result<()> {
    let tree = parser
        .parse(content, None)
        .context("Failed to parse Rust file")?;

    let root = tree.root_node();
    let bytes = content.as_bytes();

    // Traverse AST
    let mut cursor = root.walk();
    traverse_rust_ast(&mut cursor, bytes, path, map);

    Ok(())
}

/// Traverse Rust AST and extract semantic info
fn traverse_rust_ast(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    path: &Path,
    map: &mut SemanticMap,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        match kind {
            "struct_item" => {
                if let Some(def) = extract_struct(node, source, path) {
                    map.structs.push(def);
                }
            }
            "trait_item" => {
                if let Some(def) = extract_trait(node, source, path) {
                    map.traits.push(def);
                }
            }
            "function_item" => {
                if let Some(def) = extract_function(node, source, path) {
                    map.functions.push(def);
                }
            }
            "impl_item" => {
                if let Some(info) = extract_impl(node, source, path) {
                    map.implementations.push(info);
                }
            }
            "use_declaration" => {
                if let Some((from, to)) = extract_use(node, source, path) {
                    map.dependencies.push((from, to));
                }
            }
            "mod_item" => {
                if let Some(module) = extract_mod(node, source, path) {
                    map.modules.insert(module.name.clone(), module);
                }
            }
            _ => {}
        }

        // Recurse into children
        if cursor.goto_first_child() {
            traverse_rust_ast(cursor, source, path, map);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

// ============================================================================
// Node Extractors
// ============================================================================

fn extract_struct(node: tree_sitter::Node, source: &[u8], path: &Path) -> Option<StructDef> {
    let name = find_child_text(node, "type_identifier", source)?;
    let is_public = has_visibility_modifier(node);
    let line = node.start_position().row as u32 + 1;

    // Extract derives from attributes
    let derives = extract_derives(node, source);

    // Extract fields
    let mut fields = Vec::new();
    if let Some(field_list) = node.child_by_field_name("body") {
        let mut cursor = field_list.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "field_declaration" {
                    if let Some(field) = extract_field(child, source) {
                        fields.push(field);
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    Some(StructDef {
        name,
        file: path.to_path_buf(),
        line,
        is_public,
        fields,
        derives,
    })
}

fn extract_field(node: tree_sitter::Node, source: &[u8]) -> Option<FieldDef> {
    let name = find_child_text(node, "field_identifier", source)?;
    let type_name = node
        .child_by_field_name("type")
        .map(|n| node_text(n, source))
        .unwrap_or_default();
    let is_public = has_visibility_modifier(node);

    Some(FieldDef {
        name,
        type_name,
        is_public,
    })
}

fn extract_trait(node: tree_sitter::Node, source: &[u8], path: &Path) -> Option<TraitDef> {
    let name = find_child_text(node, "type_identifier", source)?;
    let is_public = has_visibility_modifier(node);
    let line = node.start_position().row as u32 + 1;

    // Extract method names
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "function_signature_item" || child.kind() == "function_item" {
                    if let Some(fn_name) = find_child_text(child, "identifier", source) {
                        methods.push(fn_name);
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    Some(TraitDef {
        name,
        file: path.to_path_buf(),
        line,
        is_public,
        methods,
    })
}

fn extract_function(node: tree_sitter::Node, source: &[u8], path: &Path) -> Option<FunctionDef> {
    let name = find_child_text(node, "identifier", source)?;
    let is_public = has_visibility_modifier(node);
    let line = node.start_position().row as u32 + 1;

    // Check for async
    let is_async = node.children(&mut node.walk()).any(|c| c.kind() == "async");

    // Extract parameters (simplified)
    let parameters = Vec::new(); // Could be expanded

    // Extract return type
    let return_type = node
        .child_by_field_name("return_type")
        .map(|n| node_text(n, source));

    Some(FunctionDef {
        name,
        file: path.to_path_buf(),
        line,
        is_public,
        is_async,
        parameters,
        return_type,
    })
}

fn extract_impl(node: tree_sitter::Node, source: &[u8], path: &Path) -> Option<ImplInfo> {
    let line = node.start_position().row as u32 + 1;

    // Get target type
    let target_type = node
        .child_by_field_name("type")
        .map(|n| node_text(n, source))
        .unwrap_or_default();

    // Get trait name if implementing a trait
    let trait_name = node
        .child_by_field_name("trait")
        .map(|n| node_text(n, source));

    // Extract method names
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "function_item" {
                    if let Some(fn_name) = find_child_text(child, "identifier", source) {
                        methods.push(fn_name);
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    Some(ImplInfo {
        target_type,
        trait_name,
        methods,
        file: path.to_path_buf(),
        line,
    })
}

fn extract_use(node: tree_sitter::Node, source: &[u8], path: &Path) -> Option<(String, String)> {
    // Get the use path
    let use_path = node
        .child_by_field_name("argument")
        .map(|n| node_text(n, source))?;

    // From module is the current file's module
    let from = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Target is first segment of use path
    let to = use_path.split("::").next().unwrap_or(&use_path).to_string();

    Some((from, to))
}

fn extract_mod(node: tree_sitter::Node, source: &[u8], path: &Path) -> Option<ModuleNode> {
    let name = find_child_text(node, "identifier", source)?;
    let is_public = has_visibility_modifier(node);

    Some(ModuleNode {
        name,
        file: path.to_path_buf(),
        is_public,
        children: Vec::new(),
        exports: Vec::new(),
    })
}

// ============================================================================
// Helpers
// ============================================================================

fn find_child_text(node: tree_sitter::Node, kind: &str, source: &[u8]) -> Option<String> {
    for child in node.children(&mut node.walk()) {
        if child.kind() == kind {
            return Some(node_text(child, source));
        }
    }

    // Search deeper for identifier types
    for child in node.children(&mut node.walk()) {
        if let Some(text) = find_child_text(child, kind, source) {
            return Some(text);
        }
    }

    None
}

fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    std::str::from_utf8(&source[node.byte_range()])
        .unwrap_or("")
        .to_string()
}

fn has_visibility_modifier(node: tree_sitter::Node) -> bool {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "visibility_modifier" {
            return true;
        }
    }
    false
}

fn extract_derives(node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut derives = Vec::new();

    // Look for attribute items before the struct
    if let Some(parent) = node.parent() {
        for sibling in parent.children(&mut parent.walk()) {
            if sibling.kind() == "attribute_item" {
                let text = node_text(sibling, source);
                if text.contains("derive") {
                    // Extract derive names (simplified)
                    if let Some(start) = text.find('(') {
                        if let Some(end) = text.rfind(')') {
                            let inner = &text[start + 1..end];
                            for part in inner.split(',') {
                                let name = part.trim();
                                if !name.is_empty() {
                                    derives.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    derives
}

/// Build module hierarchy from collected module declarations
fn build_module_hierarchy(map: &mut SemanticMap, _root: &Path) {
    // Collect children relationships first (to avoid borrow issues)
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();

    let module_names: Vec<String> = map.modules.keys().cloned().collect();

    for name in &module_names {
        if let Some(module) = map.modules.get(name) {
            let path = module.file.clone();

            // Check if other modules are in subdirectories
            for other_name in &module_names {
                if other_name != name {
                    if let Some(other_module) = map.modules.get(other_name) {
                        if other_module
                            .file
                            .starts_with(path.parent().unwrap_or(&path))
                        {
                            // This is a potential child
                            children_map
                                .entry(name.clone())
                                .or_default()
                                .push(other_name.clone());
                        }
                    }
                }
            }
        }
    }

    // Now apply the collected children
    for (name, children) in children_map {
        if let Some(module) = map.modules.get_mut(&name) {
            for child in children {
                if !module.children.contains(&child) {
                    module.children.push(child);
                }
            }
        }
    }
}

/// Extract module dependency graph from semantic map
pub fn extract_module_graph(map: &SemanticMap) -> ModuleGraph {
    let mut graph = ModuleGraph::default();

    // Add nodes for each module
    for (name, module) in &map.modules {
        graph.nodes.insert(
            name.clone(),
            ModuleGraphNode {
                name: name.clone(),
                file_count: 1, // Simplified
                is_crate_root: name == "lib" || name == "main",
            },
        );

        // Add edges for children
        for child in &module.children {
            graph.edges.push((name.clone(), child.clone()));
        }
    }

    // Add edges from dependencies
    for (from, to) in &map.dependencies {
        if !graph.edges.contains(&(from.clone(), to.clone())) {
            graph.edges.push((from.clone(), to.clone()));
        }
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_map_default() {
        let map = SemanticMap::default();
        assert!(map.structs.is_empty());
        assert!(map.traits.is_empty());
        assert!(map.functions.is_empty());
    }

    #[test]
    fn test_module_graph_extraction() {
        let mut map = SemanticMap::default();
        map.modules.insert(
            "lib".to_string(),
            ModuleNode {
                name: "lib".to_string(),
                file: PathBuf::from("src/lib.rs"),
                is_public: true,
                children: vec!["tools".to_string()],
                exports: vec![],
            },
        );

        let graph = extract_module_graph(&map);
        assert!(graph.nodes.contains_key("lib"));
    }
}
