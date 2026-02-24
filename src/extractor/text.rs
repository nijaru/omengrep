use std::sync::LazyLock;

use regex::Regex;

use crate::types::Block;

/// File extensions treated as text/documentation.
pub const TEXT_EXTENSIONS: &[&str] = &[".md", ".mdx", ".markdown", ".txt", ".rst"];

// Chunking parameters
const CHUNK_SIZE: usize = 400; // ~400 tokens target
const CHUNK_OVERLAP: usize = 50; // ~50 tokens overlap
const MIN_CHUNK_SIZE: usize = 30; // minimum tokens for a chunk

/// Sentence boundary: split after `.` `!` `?` followed by whitespace.
static SENTENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?]\s+").unwrap());

/// Fenced code block opener/closer.
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(`{3,}|~{3,})(\w+)?").unwrap());

/// Markdown header line.
static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+)$").unwrap());

/// Extract blocks from a text/documentation file.
pub fn extract_text_blocks(file_path: &str, content: &str) -> Vec<Block> {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    if matches!(ext.as_str(), ".md" | ".mdx" | ".markdown") {
        extract_markdown_blocks(file_path, content)
    } else {
        extract_plain_text_blocks(file_path, content)
    }
}

fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

fn split_text_recursive(text: &str, chunk_size: usize) -> Vec<String> {
    let separators: Vec<Option<&str>> = vec![Some("\n\n"), Some("\n"), None, Some(" ")];
    split_with_separators(text, chunk_size, &separators)
}

fn split_with_separators(
    text: &str,
    chunk_size: usize,
    separators: &[Option<&str>],
) -> Vec<String> {
    if estimate_tokens(text) <= chunk_size {
        return if text.trim().is_empty() {
            vec![]
        } else {
            vec![text.to_string()]
        };
    }

    for (i, sep) in separators.iter().enumerate() {
        let (parts, joiner) = match sep {
            None => {
                // Sentence splitting: split after sentence-ending punctuation
                let parts: Vec<&str> = SENTENCE_RE
                    .split(text)
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                if parts.len() <= 1 {
                    continue;
                }
                (parts, " ")
            }
            Some(s) => {
                if !text.contains(s) {
                    continue;
                }
                (text.split(s).collect::<Vec<_>>(), *s)
            }
        };

        let mut chunks = Vec::new();
        let mut current = String::new();

        for part in &parts {
            let candidate = if current.is_empty() {
                part.to_string()
            } else {
                format!("{current}{joiner}{part}")
            };

            if estimate_tokens(&candidate) <= chunk_size {
                current = candidate;
            } else {
                if !current.is_empty() {
                    chunks.push(current);
                }
                if estimate_tokens(part) > chunk_size && i + 1 < separators.len() {
                    chunks.extend(split_with_separators(
                        part,
                        chunk_size,
                        &separators[i + 1..],
                    ));
                    current = String::new();
                } else {
                    current = part.to_string();
                }
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        if !chunks.is_empty() {
            return chunks;
        }
    }

    // Fallback: hard split by words
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    let mut current_words = Vec::new();

    for word in words {
        current_words.push(word);
        if estimate_tokens(&current_words.join(" ")) >= chunk_size {
            chunks.push(current_words.join(" "));
            current_words.clear();
        }
    }
    if !current_words.is_empty() {
        chunks.push(current_words.join(" "));
    }
    chunks
}

fn add_overlap(chunks: &[String], overlap: usize) -> Vec<String> {
    if chunks.len() <= 1 || overlap == 0 {
        return chunks.to_vec();
    }

    let mut result = vec![chunks[0].clone()];
    for i in 1..chunks.len() {
        let prev_words: Vec<&str> = chunks[i - 1].split_whitespace().collect();
        let overlap_words = if prev_words.len() > overlap {
            &prev_words[prev_words.len() - overlap..]
        } else {
            &prev_words
        };
        let overlap_text = overlap_words.join(" ");
        result.push(format!("{overlap_text} {}", chunks[i]));
    }
    result
}

struct MarkdownSection {
    headers: Vec<String>,
    content: String,
    start_line: usize,
    end_line: usize,
    section_type: &'static str,
    language: Option<String>,
}

fn parse_markdown_structure(content: &str) -> Vec<MarkdownSection> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();
    let mut current_headers: Vec<String> = Vec::new();
    let mut current_content: Vec<&str> = Vec::new();
    let mut current_start = 0;
    let mut in_code_block = false;
    let mut code_block_start = 0;
    let mut code_block_lang: Option<String> = None;
    let mut code_block_lines: Vec<&str> = Vec::new();

    let fence_re = &*FENCE_RE;
    let header_re = &*HEADER_RE;

    let save_section = |headers: &[String],
                        content_lines: &[&str],
                        start: usize,
                        end: usize|
     -> Option<MarkdownSection> {
        let text = content_lines.join("\n");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(MarkdownSection {
            headers: headers.to_vec(),
            content: trimmed.to_string(),
            start_line: start,
            end_line: end,
            section_type: "text",
            language: None,
        })
    };

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = fence_re.captures(line) {
            if !in_code_block {
                if let Some(section) = save_section(
                    &current_headers,
                    &current_content,
                    current_start,
                    i.saturating_sub(1),
                ) {
                    sections.push(section);
                }
                current_content.clear();
                in_code_block = true;
                code_block_start = i;
                code_block_lang = caps.get(2).map(|m| m.as_str().to_string());
                code_block_lines.clear();
            } else {
                in_code_block = false;
                let code_content = code_block_lines.join("\n");
                if !code_content.trim().is_empty() {
                    sections.push(MarkdownSection {
                        headers: current_headers.clone(),
                        content: code_content,
                        start_line: code_block_start,
                        end_line: i,
                        section_type: "code",
                        language: code_block_lang.take(),
                    });
                }
                current_start = i + 1;
            }
            continue;
        }

        if in_code_block {
            code_block_lines.push(line);
            continue;
        }

        if let Some(caps) = header_re.captures(line) {
            if let Some(section) = save_section(
                &current_headers,
                &current_content,
                current_start,
                i.saturating_sub(1),
            ) {
                sections.push(section);
            }

            let level = caps.get(1).unwrap().as_str().len();
            let title = caps.get(2).unwrap().as_str().trim().to_string();
            current_headers.truncate(level - 1);
            current_headers.push(title);
            current_content.clear();
            current_start = i;
        } else {
            current_content.push(line);
        }
    }

    if let Some(section) = save_section(
        &current_headers,
        &current_content,
        current_start,
        lines.len().saturating_sub(1),
    ) {
        sections.push(section);
    }

    sections
}

fn extract_markdown_blocks(file_path: &str, content: &str) -> Vec<Block> {
    let sections = parse_markdown_structure(content);
    let mut blocks = Vec::new();

    for section in sections {
        let context = if section.headers.is_empty() {
            None
        } else {
            Some(section.headers.join(" > "))
        };

        if section.section_type == "code" {
            let lang = section.language.as_deref().unwrap_or("code");
            let prefix = match &context {
                Some(ctx) => format!("{ctx} | {lang}"),
                None => lang.to_string(),
            };
            let content_with_context = format!("{prefix}\n{}", section.content);

            blocks.push(Block {
                id: Block::make_id(file_path, section.start_line, lang),
                file: file_path.to_string(),
                block_type: "code".to_string(),
                name: lang.to_string(),
                start_line: section.start_line,
                end_line: section.end_line,
                content: content_with_context,
            });
            continue;
        }

        let chunks = split_text_recursive(&section.content, CHUNK_SIZE);
        let chunks = add_overlap(&chunks, CHUNK_OVERLAP);

        for chunk in &chunks {
            if estimate_tokens(chunk) < MIN_CHUNK_SIZE {
                continue;
            }

            let block_type = if section.headers.is_empty() {
                "text"
            } else {
                "section"
            };
            let name = section.headers.last().cloned().unwrap_or_default();
            let content_with_context = match &context {
                Some(ctx) => format!("{ctx} | {chunk}"),
                None => chunk.clone(),
            };

            blocks.push(Block {
                id: Block::make_id(file_path, section.start_line, &name),
                file: file_path.to_string(),
                block_type: block_type.to_string(),
                name,
                start_line: section.start_line,
                end_line: section.end_line,
                content: content_with_context,
            });
        }
    }

    blocks
}

fn extract_plain_text_blocks(file_path: &str, content: &str) -> Vec<Block> {
    let chunks = split_text_recursive(content, CHUNK_SIZE);
    let chunks = add_overlap(&chunks, CHUNK_OVERLAP);
    let mut blocks = Vec::new();
    let mut line_num = 0;

    for chunk in &chunks {
        if estimate_tokens(chunk) < MIN_CHUNK_SIZE {
            continue;
        }

        let chunk_lines = chunk.matches('\n').count() + 1;
        let name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("text");

        blocks.push(Block {
            id: Block::make_id(file_path, line_num, name),
            file: file_path.to_string(),
            block_type: "text".to_string(),
            name: name.to_string(),
            start_line: line_num,
            end_line: line_num + chunk_lines,
            content: chunk.clone(),
        });

        line_num += chunk_lines;
    }

    blocks
}
