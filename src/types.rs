use serde::{Deserialize, Serialize};

/// A code block extracted from a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block ID: "rel/path:start_line:name"
    pub id: String,
    /// Relative file path from index root.
    pub file: String,
    /// Block type (function, class, method, text, section, etc.)
    pub block_type: String,
    /// Name of the block (function/class name, or header text).
    pub name: String,
    /// Start line (0-indexed).
    pub start_line: usize,
    /// End line (0-indexed).
    pub end_line: usize,
    /// Source content of the block.
    pub content: String,
}

impl Block {
    pub fn make_id(file: &str, start_line: usize, name: &str) -> String {
        format!("{file}:{start_line}:{name}")
    }

    /// Text representation for embedding: "type name\ncontent"
    pub fn embedding_text(&self) -> String {
        format!("{} {}\n{}", self.block_type, self.name, self.content)
    }
}

/// A search result returned to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// File path (absolute for display, relative for JSON).
    pub file: String,
    /// Block type.
    #[serde(rename = "type")]
    pub block_type: String,
    /// Block name.
    pub name: String,
    /// Start line.
    pub line: usize,
    /// End line.
    pub end_line: usize,
    /// Source content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Similarity/relevance score.
    pub score: f32,
}

/// Parsed file reference from CLI input.
#[derive(Debug, Clone)]
pub enum FileRef {
    /// file#name — find block by name
    ByName { path: String, name: String },
    /// file:line — find block by line number
    ByLine { path: String, line: usize },
    /// file — find first block
    ByFile { path: String },
}

/// Output format for search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Default: colored terminal output with content preview.
    Default,
    /// JSON output.
    Json,
    /// Compact: no content in output.
    Compact,
    /// Files only: unique file paths.
    FilesOnly,
}

/// Stats returned from indexing operations.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub files: usize,
    pub blocks: usize,
    pub skipped: usize,
    pub errors: usize,
    pub deleted: usize,
}

/// Exit codes matching Python implementation.
pub const EXIT_MATCH: i32 = 0;
pub const EXIT_NO_MATCH: i32 = 1;
pub const EXIT_ERROR: i32 = 2;
