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
        if !self.parsers.contains_key(&ext)
            && let Some(language) = get_language(&ext)
        {
            let mut parser = Parser::new();
            parser.set_language(&language)?;
            let query = get_query_source(&ext).and_then(|qs| Query::new(&language, qs).ok());
            self.parsers.insert(ext.clone(), (parser, language, query));
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
                if is_decorated_inner_definition(&node) {
                    continue;
                }

                let range = (node.start_byte(), node.end_byte());
                if !seen_ranges.insert(range) {
                    continue;
                }

                let semantic_node = decorated_inner_definition(&node).unwrap_or(node);
                let name = extract_name(&semantic_node, content_bytes);
                let node_text = node
                    .utf8_text(content_bytes)
                    .unwrap_or_default()
                    .to_string();

                let capture_name = query.capture_names()[capture.index as usize];
                let block_type = decorated_block_type(&semantic_node).unwrap_or(capture_name);

                let start_line = node.start_position().row;
                let end_line = node.end_position().row;

                let skeleton =
                    extract_skeleton(&node, content_bytes).unwrap_or_else(|| node_text.clone());

                blocks.push(Block {
                    id: Block::make_id(rel_path, start_line, &name),
                    file: rel_path.to_string(),
                    block_type: block_type.to_string(),
                    name,
                    start_line,
                    end_line,
                    content: node_text,
                    skeleton,
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

/// Container block types that should be removed when they have children.
/// Functions/methods are NOT containers — a decorated_definition wrapping
/// a function_definition should keep the outer (decorated) block.
const CONTAINER_TYPES: &[&str] = &[
    "class",
    "struct",
    "module",
    "impl",
    "trait",
    "enum",
    "interface",
    "block",
];

/// Remove container blocks whose content is fully covered by children.
/// Only drops class/struct/module/impl parents, not function wrappers
/// like decorated_definition.
fn remove_nested_blocks(mut blocks: Vec<Block>) -> Vec<Block> {
    if blocks.len() <= 1 {
        return blocks;
    }

    // Sort by start line, then by size descending (larger blocks first)
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
        // Only container types get dropped when they have children
        if !CONTAINER_TYPES.contains(&blocks[i].block_type.as_str()) {
            continue;
        }
        for j in (i + 1)..blocks.len() {
            if blocks[j].start_line > blocks[i].end_line {
                break; // sorted — no more children possible
            }
            if !keep[j] {
                continue;
            }
            let is_contained = blocks[j].start_line >= blocks[i].start_line
                && blocks[j].end_line <= blocks[i].end_line
                && (blocks[j].start_line != blocks[i].start_line
                    || blocks[j].end_line != blocks[i].end_line);
            if !is_contained {
                continue;
            }

            keep[i] = false;
            break;
        }
    }

    blocks
        .into_iter()
        .enumerate()
        .filter_map(|(i, b)| if keep[i] { Some(b) } else { None })
        .collect()
}

fn is_decorated_inner_definition(node: &tree_sitter::Node) -> bool {
    matches!(node.kind(), "function_definition" | "class_definition")
        && node
            .parent()
            .is_some_and(|parent| parent.kind() == "decorated_definition")
}

fn decorated_inner_definition<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "decorated_definition" {
        return None;
    }

    (0..node.child_count()).find_map(|i| {
        let child = node.child(i as u32)?;
        matches!(child.kind(), "function_definition" | "class_definition").then_some(child)
    })
}

fn decorated_block_type(node: &tree_sitter::Node) -> Option<&'static str> {
    match node.kind() {
        "function_definition" => Some("function"),
        "class_definition" => Some("class"),
        _ => None,
    }
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
        if let Some(child) = node.child(i as u32)
            && name_types.contains(&child.kind())
            && let Ok(text) = child.utf8_text(source)
        {
            return text.to_string();
        }
    }

    // Search one level deeper
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            for j in 0..child.child_count() {
                if let Some(grandchild) = child.child(j as u32)
                    && name_types.contains(&grandchild.kind())
                    && let Ok(text) = grandchild.utf8_text(source)
                {
                    return text.to_string();
                }
            }
        }
    }

    "anonymous".to_string()
}

/// Extract a skeleton representation of a block by omitting its body node.
fn extract_skeleton(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let body_types = [
        "block",
        "statement_block",
        "class_body",
        "declaration_list",
        "compound_statement",
        "field_declaration_list",
        "interface_body",
        "body_statement",
        "do_block",
        "enum_body_item",
        "struct_body_item",
    ];

    let mut body_node = None;
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            let kind = child.kind();
            if body_types.contains(&kind) {
                body_node = Some(child);
                break;
            }
        }
    }

    let body = body_node?;

    let start_byte = node.start_byte();
    let body_start = body.start_byte();
    let body_end = body.end_byte();
    let end_byte = node.end_byte();

    let mut skeleton = Vec::new();
    skeleton.extend_from_slice(&source[start_byte..body_start]);

    let body_text = body.utf8_text(source).unwrap_or("");
    if body_text.starts_with('{') && body_text.ends_with('}') {
        skeleton.extend_from_slice(b"{ ... }");
    } else if body_text.starts_with(':') {
        skeleton.extend_from_slice(b": ...");
    } else {
        skeleton.extend_from_slice(b"...");
    }

    skeleton.extend_from_slice(&source[body_end..end_byte]);

    String::from_utf8(skeleton).ok()
}

/// Fallback: return first 50 lines as a single block.
fn fallback_head(file_path: &str, content: &str) -> Vec<Block> {
    let end_byte = content
        .match_indices('\n')
        .nth(49)
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let fallback_content = &content[..end_byte];

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
        end_line: fallback_content.lines().count().saturating_sub(1),
        content: fallback_content.to_string(),
        skeleton: fallback_content.to_string(),
    }]
}

#[cfg(test)]
mod tests {
    use crate::extractor::Extractor;

    #[test]
    fn decorated_python_function_uses_inner_name() {
        let mut extractor = Extractor::new();
        let blocks = extractor
            .extract(
                "queue.py",
                r#"
from celery import Celery

app = Celery("myapp")

@task
def send_async_email(email_address: str, subject: str):
    print(subject)
"#,
            )
            .unwrap();

        let matching: Vec<_> = blocks
            .iter()
            .filter(|block| block.name == "send_async_email")
            .collect();

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].start_line, 5);
        assert_eq!(matching[0].block_type, "function");
    }

    #[test]
    fn decorated_python_class_uses_inner_name_and_type() {
        let mut extractor = Extractor::new();
        let blocks = extractor
            .extract(
                "models.py",
                r#"
def dataclass(cls):
    return cls

@dataclass
class DecoratedThing:
    pass
"#,
            )
            .unwrap();

        let matching: Vec<_> = blocks
            .iter()
            .filter(|block| block.name == "DecoratedThing")
            .collect();

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].start_line, 4);
        assert_eq!(matching[0].block_type, "class");
    }
}
