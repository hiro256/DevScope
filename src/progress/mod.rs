//! Project progress analysis independent of the TUI.

mod markdown;

pub use markdown::{
    MarkdownProgress, MarkdownProgressError, MarkdownTask, analyze_markdown_progress,
    discover_markdown_files,
};
