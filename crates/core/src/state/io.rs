//! # IO Utilities
//!
//! File system operations for the `.catalyst` runtime directory.
//! All harness-related code has been removed - specs and prompts now live in the database.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Get the runtime directory path (.catalyst)
///
/// This is the primary storage location for all Catalyst runtime files.
pub fn get_runtime_path() -> PathBuf {
    // Check for environment variable override
    if let Ok(path) = std::env::var("CATALYST_RUNTIME_PATH") {
        return PathBuf::from(path);
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".catalyst")
}

/// Ensure the runtime directory exists
pub async fn ensure_runtime_dir() -> Result<PathBuf> {
    let path = get_runtime_path();
    fs::create_dir_all(&path)
        .await
        .with_context(|| format!("Failed to create runtime directory: {:?}", path))?;
    Ok(path)
}

/// Read a file from the runtime directory
pub async fn read_file(relative_path: impl AsRef<Path>) -> Result<String> {
    let path = get_runtime_path().join(relative_path.as_ref());
    fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read file: {:?}", path))
}

/// Write a file to the runtime directory
pub async fn write_runtime_file(relative_path: impl AsRef<Path>, content: &str) -> Result<()> {
    let path = get_runtime_path().join(relative_path);

    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(&path, content)
        .await
        .with_context(|| format!("Failed to write file: {:?}", path))
}

/// Check if a runtime file exists
pub async fn file_exists(relative_path: impl AsRef<Path>) -> bool {
    let path = get_runtime_path().join(relative_path);
    fs::metadata(&path).await.is_ok()
}

/// List files in a runtime subdirectory
pub async fn list_runtime_files(subdir: &str) -> Result<Vec<String>> {
    let dir = get_runtime_path().join(subdir);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(&dir)
        .await
        .with_context(|| format!("Failed to read directory: {:?}", dir))?;

    let mut files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        if let Ok(file_type) = entry.file_type().await {
            if file_type.is_file() {
                if let Ok(name) = entry.file_name().into_string() {
                    files.push(name);
                }
            }
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_runtime_path() {
        let path = get_runtime_path();
        assert!(path.ends_with(".catalyst"));
    }

    #[tokio::test]
    async fn test_file_operations() {
        let test_path = "test_io_file.txt";
        let content = "Hello, Catalyst!";

        // Ensure runtime dir exists
        let _ = ensure_runtime_dir().await;

        // Write
        write_runtime_file(test_path, content).await.unwrap();

        // Check exists
        assert!(file_exists(test_path).await);

        // Read
        let read_content = read_file(test_path).await.unwrap();
        assert_eq!(read_content, content);

        // Cleanup
        let full_path = get_runtime_path().join(test_path);
        let _ = fs::remove_file(full_path).await;
    }
}
