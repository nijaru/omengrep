pub mod languages;
pub mod queries;
pub mod text;

use std::path::Path;

use anyhow::Result;
use tree_sitter::{Language, Parser, Query, StreamingIterator};

use crate::types::Block;

use languages::get_language;
use queries::get_query_source;
use text::TEXT_EXTENSIONS;

/// Extracts code blocks from source files using tree-sitter.
pub struct Extractor {
    /// Cached parsers per extension.
    parsers: std::collections::HashMap<String, (Parser, Language, Option<Query>)>,
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            parsers: std::collections::HashMap::new(),
        }
    }

    /// Extract blocks from a file.
    pub fn extract(&mut self, file_path: &str, content: &str) -> Result<Vec<Block>> {
        let ext = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_default();

        let rel_path = file_path;

        // Text/doc files: use chunk-based extraction
        if TEXT_EXTENSIONS.contains(&ext.as_str()) {
            return Ok(text::extract_text_blocks(file_path, content));
        }

        // Ensure parser is initialized for this extension
        if !self.parsers.contains_key(&ext) {
            if let Some(language) = get_language(&ext) {
                let mut parser = Parser::new();
                parser.set_language(&language)?;
                let query = get_query_source(&ext).and_then(|qs| Query::new(&language, qs).ok());
                self.parsers.insert(ext.clone(), (parser, language, query));
            }
        }

        let Some((parser, _language, query)) = self.parsers.get_mut(&ext) else {
            return Ok(fallback_head(rel_path, content));
        };

        let Some(query) = query else {
            return Ok(fallback_head(rel_path, content));
        };

        let content_bytes = content.as_bytes();
        let Some(tree) = parser.parse(content_bytes, None) else {
            return Ok(fallback_head(rel_path, content));
        };

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), content_bytes);

        let mut blocks = Vec::new();
        let mut seen_ranges = std::collections::HashSet::new();

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;
                let range = (node.start_byte(), node.end_byte());
                if !seen_ranges.insert(range) {
                    continue;
                }

                let name = extract_name(&node, content_bytes);
                let node_content = &content_bytes[node.start_byte()..node.end_byte()];
                let node_text = String::from_utf8_lossy(node_content).into_owned();

                let capture_name = query.capture_names()[capture.index as usize];
                let block_type = capture_name;

                let start_line = node.start_position().row;
                let end_line = node.end_position().row;

                blocks.push(Block {
                    id: Block::make_id(rel_path, start_line, &name),
                    file: rel_path.to_string(),
                    block_type: block_type.to_string(),
                    name,
                    start_line,
                    end_line,
                    content: node_text,
                });
            }
        }

        if blocks.is_empty() {
            return Ok(fallback_head(rel_path, content));
        }

        // Remove outer blocks whose content is fully covered by inner blocks.
        // E.g., a class block contains all its method blocks — keep methods, drop class.
        blocks = remove_nested_blocks(blocks);

        Ok(blocks)
    }
}

/// Remove blocks that are fully contained within other blocks.
/// When a parent block (e.g., class) contains children (e.g., methods),
/// drop the parent to avoid duplicate content in the index.
fn remove_nested_blocks(mut blocks: Vec<Block>) -> Vec<Block> {
    if blocks.len() <= 1 {
        return blocks;
    }

    // Sort by start byte (start_line as proxy), then by size descending
    blocks.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(b.end_line.cmp(&a.end_line))
    });

    let mut keep = vec![true; blocks.len()];

    for i in 0..blocks.len() {
        if !keep[i] {
            continue;
        }
        // Check if block i fully contains any later blocks
        let mut has_children = false;
        for j in (i + 1)..blocks.len() {
            if !keep[j] {
                continue;
            }
            if blocks[j].start_line >= blocks[i].start_line
                && blocks[j].end_line <= blocks[i].end_line
                && (blocks[j].start_line != blocks[i].start_line
                    || blocks[j].end_line != blocks[i].end_line)
            {
                has_children = true;
            }
        }
        if has_children {
            keep[i] = false;
        }
    }

    blocks
        .into_iter()
        .enumerate()
        .filter_map(|(i, b)| if keep[i] { Some(b) } else { None })
        .collect()
}

/// Extract the name identifier from a tree-sitter node.
fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> String {
    let name_types = [
        "identifier",
        "name",
        "field_identifier",
        "type_identifier",
        "constant",
        "simple_identifier",
        "word",
    ];

    // Search direct children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if name_types.contains(&child.kind()) {
                if let Ok(text) = child.utf8_text(source) {
                    return text.to_string();
                }
            }
        }
    }

    // Search one level deeper
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            for j in 0..child.child_count() {
                if let Some(grandchild) = child.child(j) {
                    if name_types.contains(&grandchild.kind()) {
                        if let Ok(text) = grandchild.utf8_text(source) {
                            return text.to_string();
                        }
                    }
                }
            }
        }
    }

    "anonymous".to_string()
}

/// Fallback: return first 50 lines as a single block.
fn fallback_head(file_path: &str, content: &str) -> Vec<Block> {
    let lines: Vec<&str> = content.lines().take(50).collect();
    let end_line = lines.len().saturating_sub(1);
    let name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    vec![Block {
        id: Block::make_id(file_path, 0, name),
        file: file_path.to_string(),
        block_type: "file".to_string(),
        name: name.to_string(),
        start_line: 0,
        end_line,
        content: lines.join("\n"),
    }]
}
