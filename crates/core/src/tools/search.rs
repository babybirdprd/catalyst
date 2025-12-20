//! # Search - Symbol Query Tool
//!
//! Provides search capabilities over the symbol index.

use super::scanner::{SymbolIndex, SymbolInfo, SymbolKind};

/// Find a symbol by exact name
pub fn find_definition<'a>(index: &'a SymbolIndex, symbol: &str) -> Option<&'a SymbolInfo> {
    index.find_by_name(symbol).into_iter().next()
}

/// Search for symbols matching a pattern
pub fn search_symbols<'a>(index: &'a SymbolIndex, query: &str) -> Vec<&'a SymbolInfo> {
    index.search(query)
}

/// Find all functions in the index
pub fn find_functions(index: &SymbolIndex) -> Vec<&SymbolInfo> {
    index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .collect()
}

/// Find all structs/types in the index
pub fn find_types(index: &SymbolIndex) -> Vec<&SymbolInfo> {
    index
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Type
            )
        })
        .collect()
}

/// Get symbols from a specific file
pub fn symbols_in_file<'a>(index: &'a SymbolIndex, file: &str) -> Vec<&'a SymbolInfo> {
    index
        .symbols
        .iter()
        .filter(|s| s.file.to_string_lossy().contains(file))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_find_definition() {
        let mut index = SymbolIndex::new();
        index.add(SymbolInfo {
            name: "MyStruct".to_string(),
            path: "MyStruct".to_string(),
            kind: SymbolKind::Struct,
            file: PathBuf::from("lib.rs"),
            line: 10,
            signature: None,
        });

        let result = find_definition(&index, "MyStruct");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "MyStruct");
    }
}
