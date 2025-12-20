# State Management

Catalyst employs a **Hybrid State Model** to balance durability, queryability, and human-readability.

## 1. CatalystDb (SQLite)

The `CatalystDb` is the primary source of truth for structured data that requires transactional integrity and efficient querying.

*   **File**: `.catalyst/catalyst.db`
*   **Technology**: `rusqlite` (bundled)
*   **Schema**:
    *   `kv_store`: Generic key-value store for simple settings.
    *   `interactions`: Stores human-in-the-loop requests (questions, approvals) and their responses.
    *   `features`: (Legacy/Migration) Feature tracking.
    *   `embeddings`: (Future) Vector store for semantic search.

The `InteractionManager` uses this DB to persist questions asked to the user, ensuring that the system can be restarted without losing pending user input.

## 2. Project State (JSON)

The `ProjectState` manages the high-level configuration and metadata of the project. It is designed to be easily machine-parsable.

*   **File**: `state.json` (root or `.catalyst/`)
*   **Struct**: `crates/core/src/state/project_state.rs`
*   **Contents**:
    *   `project_id`: Unique identifier.
    *   `mode`: `SpeedRun`, `Lab`, or `Fortress`.
    *   `stack`: Tech stack details (detected or configured).
    *   `metadata`: Versioning and timestamps.

## 3. Specifications (Markdown)

The `SpecManager` handles the human-readable intent of the project. We use Markdown so that the "Brain" of the project is accessible to developers.

*   **File**: `spec.md`
*   **Format**: A structured Markdown file with sections for:
    *   `# Context`: High-level goals.
    *   `# Unknowns`: Resolved ambiguities.
    *   `# Architecture`: Selected patterns and libraries.
    *   `# Features`: Implementation status.
*   **Fragments**: The `SpecManager` can read/write specific sections ("fragments") of the spec without overwriting the whole file.

## 4. Feature Manager (Sharded JSON)

To avoid merge conflicts and allow parallel processing, individual features are sharded into their own files.

*   **Directory**: `.catalyst/features/`
*   **Format**: JSON files named `{feature_id}.json`.
*   **Manager**: `crates/core/src/state/feature_state.rs`

### Feature Lifecycle
1.  **Pending**: Identified by Atomizer but not started.
2.  **Building**: Currently being implemented by a Builder in a worktree.
3.  **Testing**: Code written, running tests.
4.  **Merging**: Tests passed, merge in progress.
5.  **Complete**: Merged to main branch.
6.  **Failed**: Build or tests failed, requires human intervention or retry.

## Snapshotting

The `SnapshotManager` provides a safety mechanism by creating backups of the state before critical operations.

*   **Directory**: `.catalyst/snapshots/`
*   **Triggers**: Before running the `Builder`, before applying a merge.
*   **Recovery**: Allows rolling back `state.json` and `spec.md` if an agent hallucinates or corrupts the state.
