//! # Memory Module
//!
//! Semantic memory for agent knowledge persistence using Radkit's memory system.
//!
//! ## Architecture
//!
//! ```text
//! OwnedHistory (conversations) + OwnedKnowledge (documents)
//!                    ↓
//!              MemoryService
//!                    ↓
//!         SqliteMemoryService (persistent) or InMemoryMemoryService (radkit)
//! ```

pub mod catalyst_memory;
pub mod sqlite_memory;

pub use catalyst_memory::{CatalystMemory, MemoryConfig, MemoryProvider};
pub use sqlite_memory::SqliteMemoryService;
