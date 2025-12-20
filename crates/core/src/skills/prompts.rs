//! Default prompt templates bundled at compile time.
//!
//! These are used for seeding the database on first run.
//! At runtime, prompts are loaded from the database to allow customization.

/// Unknowns Parser - identifies ambiguities in user goals
pub const UNKNOWNS_PARSER: &str = include_str!("defaults/unknowns_parser.md");

/// Researcher - finds solutions to technical unknowns
pub const RESEARCHER: &str = include_str!("defaults/researcher.md");

/// Architect - makes binding technology decisions
pub const ARCHITECT: &str = include_str!("defaults/architect.md");

/// Critic - security and quality auditor
pub const CRITIC: &str = include_str!("defaults/critic.md");

/// Atomizer - breaks features into modular chunks
pub const ATOMIZER: &str = include_str!("defaults/atomizer.md");

/// Taskmaster - generates mission prompts for coding agents
pub const TASKMASTER: &str = include_str!("defaults/taskmaster.md");

/// Red Team - writes hostile tests before implementation
pub const RED_TEAM: &str = include_str!("defaults/red_team.md");

/// Gardener - codebase maintenance and cleanup
pub const GARDENER: &str = include_str!("defaults/gardener.md");

/// Builder - final verification and repair agent
pub const BUILDER: &str = include_str!("defaults/builder.md");

/// Drafter - single-file code generation
pub const DRAFTER: &str = include_str!("defaults/drafter.md");

/// WebScraper - HTML content extraction
pub const WEBSCRAPER: &str = include_str!("defaults/webscraper.md");

/// Merge - 3-way merge conflict resolution
pub const MERGE: &str = include_str!("defaults/merge.md");

/// All default prompts with their slugs for seeding
pub fn all_defaults() -> Vec<(&'static str, &'static str)> {
    vec![
        ("unknowns_parser", UNKNOWNS_PARSER),
        ("researcher", RESEARCHER),
        ("architect", ARCHITECT),
        ("critic", CRITIC),
        ("atomizer", ATOMIZER),
        ("taskmaster", TASKMASTER),
        ("red_team", RED_TEAM),
        ("gardener", GARDENER),
        ("builder", BUILDER),
        ("drafter", DRAFTER),
        ("webscraper", WEBSCRAPER),
        ("merge", MERGE),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_prompts_non_empty() {
        for (slug, content) in all_defaults() {
            assert!(!content.is_empty(), "Prompt '{}' should not be empty", slug);
            assert!(content.len() > 50, "Prompt '{}' seems too short", slug);
        }
    }

    #[test]
    fn test_prompt_count() {
        assert_eq!(all_defaults().len(), 12, "Should have 12 default prompts");
    }
}
