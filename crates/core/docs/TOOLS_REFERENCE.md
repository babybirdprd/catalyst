# Tools Reference

Catalyst follows the "Cyborg" philosophy: **Code > Agents**. Whenever possible, we use deterministic Rust code to handle tasks rather than relying on LLM hallucinations. These tools are located in `crates/core/src/tools`.

## 1. Git Tool (`git.rs`)

The Git tool manages the complexity of worktree isolation.

### Key Functions
*   `create_worktree(root, feature_id)`: Creates a new, isolated directory linked to the main repository. This allows agents to modify files without affecting the main working copy.
*   `merge_worktree(root, feature_id)`: Merges the worktree back into the main branch.
*   `delete_worktree(root, feature_id)`: Cleans up the worktree after success or failure.

### Safety Checks
*   Ensures the repo is clean before creating a worktree.
*   Handles merge conflicts by returning a `MergeResult::Conflicts` enum, which triggers the `Merging` stage in the pipeline.

## 2. AST Scanner (`ast_scanner.rs`)

We use `tree-sitter` to perform semantic analysis of the codebase. This allows agents to "see" the code structure without reading every file (which would blow the context window).

### Capabilities
*   **Symbol Extraction**: Finds all structs, functions, and impl blocks in a file.
*   **Dependency Graph**: Maps imports and exports to understand module relationships.
*   **Language Support**: Currently supports Rust (`tree-sitter-rust`).

### Usage
Used by the `Architect` and `Builder` to understand existing code before proposing changes.

## 3. Terminal (`terminal.rs`)

A wrapper around `std::process::Command` that makes shell execution safe for agents.

### Features
*   **Structured Output**: Parses stdout/stderr into a JSON object.
*   **Timeout Enforced**: Prevents commands from hanging indefinitely.
*   **Working Directory**: Ensures commands run in the correct worktree.

### Common Commands
*   `cargo build`: Verifies compilation.
*   `cargo test`: Verifies correctness.
*   `cargo clippy`: Enforces lints.

## 4. Search (`search.rs`)

Provides semantic search capabilities over the codebase and documentation.

*   **Implementation**: Currently uses `ripgrep` (via `grep` tool wrapper) for fast regex search.
*   **Future**: Will integrate with `catalyst_core::memory` for vector embeddings search.

## 5. Linter (`linter.rs`)

Enforces the **Rule of 100** and other strict constraints.

*   **Logic**: Scans files to count lines of code (LOC).
*   **Constraint**: If a file exceeds 150 lines (soft limit), it flags it for the `Atomizer` or `Builder` to refactor.
*   **Philosophy**: Keeps modules small and agent-understandable.
