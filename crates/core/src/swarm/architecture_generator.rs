//! # Architecture Generator
//!
//! Generates `architecture.md` from semantic codebase analysis.
//! Creates a human-readable overview of module hierarchy and public APIs.

use crate::state::CodebaseProfile;
use crate::tools::ast_scanner::{extract_module_graph, SemanticMap};
use anyhow::Result;

/// Generate architecture documentation from codebase profile
///
/// Creates markdown with:
/// - Module hierarchy (mermaid diagram)
/// - Public API surface
/// - Dependency overview
pub fn generate_architecture(profile: &CodebaseProfile, semantic: &SemanticMap) -> Result<String> {
    let mut md = String::new();

    // Header
    md.push_str("# Architecture Overview\n\n");
    md.push_str(&format!(
        "_Auto-generated from {} files ({} LOC)_\n\n",
        profile.total_files, profile.total_loc
    ));

    // Project Info
    md.push_str("## Project Info\n\n");
    md.push_str(&format!("- **Type:** {:?}\n", profile.project_type));
    if !profile.frameworks.is_empty() {
        md.push_str("- **Frameworks:** ");
        let names: Vec<_> = profile.frameworks.iter().map(|f| f.name.as_str()).collect();
        md.push_str(&names.join(", "));
        md.push_str("\n");
    }
    md.push_str("\n");

    // Module Hierarchy
    md.push_str("## Module Hierarchy\n\n");
    let graph = extract_module_graph(semantic);

    if !graph.nodes.is_empty() {
        md.push_str("```mermaid\ngraph TD\n");
        for (from, to) in &graph.edges {
            md.push_str(&format!("    {}[{}] --> {}[{}]\n", from, from, to, to));
        }
        // Add isolated nodes
        for (name, node) in &graph.nodes {
            let has_edges = graph.edges.iter().any(|(f, t)| f == name || t == name);
            if !has_edges {
                let style = if node.is_crate_root { "((root))" } else { "" };
                md.push_str(&format!("    {}[{}{}]\n", name, name, style));
            }
        }
        md.push_str("```\n\n");
    } else {
        md.push_str("_No module hierarchy detected_\n\n");
    }

    // Public API Surface
    md.push_str("## Public API Surface\n\n");

    // Structs
    let public_structs: Vec<_> = semantic.structs.iter().filter(|s| s.is_public).collect();
    if !public_structs.is_empty() {
        md.push_str("### Structs\n\n");
        for s in &public_structs {
            let derives = if s.derives.is_empty() {
                String::new()
            } else {
                format!(" _(derives: {})_", s.derives.join(", "))
            };
            md.push_str(&format!(
                "- `{}` ({}:{}){}\n",
                s.name,
                s.file.file_name().unwrap_or_default().to_string_lossy(),
                s.line,
                derives
            ));
        }
        md.push_str("\n");
    }

    // Traits
    let public_traits: Vec<_> = semantic.traits.iter().filter(|t| t.is_public).collect();
    if !public_traits.is_empty() {
        md.push_str("### Traits\n\n");
        for t in &public_traits {
            let methods = if t.methods.is_empty() {
                String::new()
            } else {
                format!(" — methods: {}", t.methods.join(", "))
            };
            md.push_str(&format!("- `{}`{}\n", t.name, methods));
        }
        md.push_str("\n");
    }

    // Functions
    let public_functions: Vec<_> = semantic
        .functions
        .iter()
        .filter(|f| f.is_public)
        .take(20) // Limit to avoid huge lists
        .collect();
    if !public_functions.is_empty() {
        md.push_str("### Public Functions\n\n");
        for f in &public_functions {
            let async_marker = if f.is_async { "async " } else { "" };
            md.push_str(&format!("- `{}fn {}()`\n", async_marker, f.name));
        }
        if semantic.functions.iter().filter(|f| f.is_public).count() > 20 {
            md.push_str(&format!(
                "\n_...and {} more_\n",
                semantic.functions.iter().filter(|f| f.is_public).count() - 20
            ));
        }
        md.push_str("\n");
    }

    // Dependencies
    md.push_str("## Dependencies\n\n");
    if !profile.frameworks.is_empty() {
        md.push_str("| Crate | Category |\n");
        md.push_str("|-------|----------|\n");
        for f in &profile.frameworks {
            md.push_str(&format!("| {} | {} |\n", f.name, f.category));
        }
        md.push_str("\n");
    } else {
        md.push_str("_No framework dependencies detected_\n\n");
    }

    // Style Patterns
    md.push_str("## Code Style\n\n");
    if let Some(ref naming) = profile.style_patterns.naming_convention {
        md.push_str(&format!("- **Naming:** {}\n", naming));
    }
    if let Some(ref errors) = profile.style_patterns.error_handling {
        md.push_str(&format!("- **Error Handling:** {}\n", errors));
    }
    if let Some(ref runtime) = profile.style_patterns.async_runtime {
        md.push_str(&format!("- **Async Runtime:** {}\n", runtime));
    }
    if let Some(ref logging) = profile.style_patterns.logging {
        md.push_str(&format!("- **Logging:** {}\n", logging));
    }
    md.push_str("\n");

    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::codebase_profile::ProjectType;
    use std::path::PathBuf;

    #[test]
    fn test_generate_empty_architecture() {
        let profile = CodebaseProfile::new(PathBuf::from("/test"));
        let semantic = SemanticMap::default();

        let result = generate_architecture(&profile, &semantic).unwrap();
        assert!(result.contains("# Architecture Overview"));
        assert!(result.contains("Module Hierarchy"));
    }
}
