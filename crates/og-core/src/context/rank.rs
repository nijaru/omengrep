//! Ranked file/symbol context. Ported verbatim from og v0.0.3
//! cli/context.rs ranking policy: definition weights + cross-file inbound
//! reference scoring, high-frequency name filtering, count truncation.
//!
//! Only the block source changed: rows come from the SQLite catalog instead
//! of omendb metadata lookups. Scoring math is untouched.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

const DOC_BLOCK_TYPES: &[&str] = &["text", "section"];
const SYMBOL_REF_SCORE_CAP: usize = 12;
const SYMBOL_FILE_SCORE_CAP: usize = 8;
const FILE_REF_SCORE_CAP: usize = 40;
const FILE_FILE_SCORE_CAP: usize = 20;

/// A block loaded from the catalog for ranking (0-indexed lines).
#[derive(Clone)]
pub struct IndexedBlock {
    pub file: String,
    pub name: String,
    pub block_type: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub skeleton: String,
}

impl IndexedBlock {
    /// Doc/prose blocks never carry symbol signal.
    pub fn is_symbol(&self) -> bool {
        !DOC_BLOCK_TYPES.contains(&self.block_type.as_str())
    }
}

#[derive(Default)]
struct SymbolScore {
    inbound_refs: usize,
    inbound_files: HashSet<String>,
}

#[derive(Default)]
struct FileScore {
    definition_score: f32,
    inbound_refs: usize,
    inbound_files: HashSet<String>,
    symbols: Vec<RankedSymbol>,
}

#[derive(Clone, Serialize)]
pub struct RankedSymbol {
    pub name: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub line: usize,
    pub end_line: usize,
    pub score: f32,
    pub inbound_refs: usize,
    pub inbound_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<String>,
}

#[derive(Serialize)]
pub struct RankedFile {
    pub file: String,
    pub score: f32,
    pub definition_score: f32,
    pub inbound_refs: usize,
    pub inbound_files: usize,
    pub symbols: Vec<RankedSymbol>,
}

pub fn rank_context(
    blocks: &[IndexedBlock],
    num_files: usize,
    symbols_per_file: usize,
    include_skeleton: bool,
) -> Vec<RankedFile> {
    let mut definitions: HashMap<String, Vec<usize>> = HashMap::new();
    let mut symbol_scores: Vec<SymbolScore> =
        (0..blocks.len()).map(|_| SymbolScore::default()).collect();

    for (idx, block) in blocks.iter().enumerate() {
        if let Some(name) = symbol_key(&block.name) {
            definitions.entry(name).or_default().push(idx);
        }
    }

    let token_doc_counts = token_document_counts(blocks);
    let max_doc_freq = (blocks.len() / 40).clamp(8, 50);
    definitions.retain(|name, _| token_doc_counts.get(name).copied().unwrap_or(0) <= max_doc_freq);

    let mut seen_edges: HashSet<(usize, usize)> = HashSet::new();
    for (source_idx, block) in blocks.iter().enumerate() {
        for token in identifier_tokens(&block.content) {
            let Some(targets) = definitions.get(&token) else {
                continue;
            };

            for &target_idx in targets {
                if target_idx == source_idx || blocks[target_idx].file == block.file {
                    continue;
                }
                if !seen_edges.insert((source_idx, target_idx)) {
                    continue;
                }

                let target_score = &mut symbol_scores[target_idx];
                target_score.inbound_refs += 1;
                target_score.inbound_files.insert(block.file.clone());
            }
        }
    }

    let mut file_scores: HashMap<String, FileScore> = HashMap::new();
    for (idx, block) in blocks.iter().enumerate() {
        let definition_score = definition_weight(block);
        let symbol_score = &symbol_scores[idx];
        let score = definition_score
            + (symbol_score.inbound_refs.min(SYMBOL_REF_SCORE_CAP) as f32 * 2.0)
            + (symbol_score.inbound_files.len().min(SYMBOL_FILE_SCORE_CAP) as f32 * 3.0);

        let file_score = file_scores.entry(block.file.clone()).or_default();
        file_score.definition_score += definition_score;
        file_score.inbound_refs += symbol_score.inbound_refs;
        file_score
            .inbound_files
            .extend(symbol_score.inbound_files.iter().cloned());
        file_score.symbols.push(RankedSymbol {
            name: block.name.clone(),
            block_type: block.block_type.clone(),
            line: block.start_line + 1,
            end_line: block.end_line + 1,
            score,
            inbound_refs: symbol_score.inbound_refs,
            inbound_files: symbol_score.inbound_files.len(),
            skeleton: include_skeleton.then(|| block.skeleton.clone()),
        });
    }

    let mut ranked: Vec<RankedFile> = file_scores
        .into_iter()
        .map(|(file, mut score)| {
            score.symbols.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.line.cmp(&b.line))
            });
            score.symbols.truncate(symbols_per_file);

            let total_score = score.definition_score
                + (score.inbound_refs.min(FILE_REF_SCORE_CAP) as f32 * 2.0)
                + (score.inbound_files.len().min(FILE_FILE_SCORE_CAP) as f32 * 3.0);

            RankedFile {
                file,
                score: total_score,
                definition_score: score.definition_score,
                inbound_refs: score.inbound_refs,
                inbound_files: score.inbound_files.len(),
                symbols: score.symbols,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.file.cmp(&b.file))
    });
    ranked.truncate(num_files);
    ranked
}

fn definition_weight(block: &IndexedBlock) -> f32 {
    let base = match block.block_type.as_str() {
        "class" | "struct" | "enum" | "trait" | "interface" => 3.0,
        "module" | "namespace" => 2.5,
        "impl" | "constructor" => 2.0,
        "function" | "method" => 1.5,
        _ => 1.0,
    };

    let public_bonus = if block.content.trim_start().starts_with("pub ")
        || block.content.trim_start().starts_with("export ")
        || block.name.chars().next().is_some_and(char::is_uppercase)
    {
        0.5
    } else {
        0.0
    };

    base + public_bonus
}

fn symbol_key(name: &str) -> Option<String> {
    let name = name
        .rsplit(['.', ':', '#', '/', '\\'])
        .next()
        .unwrap_or(name)
        .trim();
    let key = name.to_ascii_lowercase();
    if is_noisy_name(&key) { None } else { Some(key) }
}

fn is_noisy_name(name: &str) -> bool {
    matches!(
        name,
        "" | "clone"
            | "debug"
            | "default"
            | "display"
            | "error"
            | "fmt"
            | "from"
            | "get"
            | "append"
            | "data"
            | "item"
            | "items"
            | "init"
            | "into"
            | "list"
            | "main"
            | "method"
            | "new"
            | "path"
            | "run"
            | "set"
            | "source"
            | "test"
            | "tests"
            | "value"
            | "values"
    ) || name.len() < 4
}

fn token_document_counts(blocks: &[IndexedBlock]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for block in blocks {
        for token in identifier_tokens(&block.content) {
            *counts.entry(token).or_insert(0) += 1;
        }
    }
    counts
}

fn identifier_tokens(text: &str) -> HashSet<String> {
    let mut tokens = HashSet::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            current.push(ch);
        } else {
            push_identifier(&mut tokens, &current);
            current.clear();
        }
    }
    push_identifier(&mut tokens, &current);

    tokens
}

fn push_identifier(tokens: &mut HashSet<String>, ident: &str) {
    if ident.len() < 4
        || !ident
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
    {
        return;
    }

    let token = ident.to_ascii_lowercase();
    if !is_noisy_name(&token) {
        tokens.insert(token);
    }
}

#[cfg(test)]
mod tests {
    use super::{IndexedBlock, rank_context};

    fn block(file: &str, name: &str, content: &str) -> IndexedBlock {
        IndexedBlock {
            file: file.to_string(),
            name: name.to_string(),
            block_type: "function".to_string(),
            start_line: 0,
            end_line: 0,
            content: content.to_string(),
            skeleton: content.to_string(),
        }
    }

    #[test]
    fn context_filters_high_frequency_definition_names() {
        let mut blocks = vec![block("defs.rs", "sharedThing", "fn sharedThing() {}")];
        for i in 0..120 {
            blocks.push(block(
                &format!("file_{i}.rs"),
                &format!("caller_{i}"),
                "fn caller() { sharedThing(); }",
            ));
        }

        let ranked = rank_context(&blocks, 1, 1, false);
        let defs = ranked.iter().find(|file| file.file == "defs.rs").unwrap();

        assert_eq!(defs.inbound_refs, 0);
        assert_eq!(defs.inbound_files, 0);
    }
}
