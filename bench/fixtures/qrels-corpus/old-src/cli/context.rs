use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;
use serde::Serialize;

use crate::index::{VECTORS_DIR, find_index_root, manifest::Manifest};
use crate::types::EXIT_ERROR;

const DOC_BLOCK_TYPES: &[&str] = &["text", "section"];
const SYMBOL_REF_SCORE_CAP: usize = 12;
const SYMBOL_FILE_SCORE_CAP: usize = 8;
const FILE_REF_SCORE_CAP: usize = 40;
const FILE_FILE_SCORE_CAP: usize = 20;

#[derive(Clone)]
struct IndexedBlock {
    file: String,
    name: String,
    block_type: String,
    start_line: usize,
    end_line: usize,
    content: String,
    skeleton: String,
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
struct RankedSymbol {
    name: String,
    #[serde(rename = "type")]
    block_type: String,
    line: usize,
    end_line: usize,
    score: f32,
    inbound_refs: usize,
    inbound_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    skeleton: Option<String>,
}

#[derive(Serialize)]
struct RankedFile {
    file: String,
    score: f32,
    definition_score: f32,
    inbound_refs: usize,
    inbound_files: usize,
    symbols: Vec<RankedSymbol>,
}

pub fn run(
    path: &Path,
    num_files: usize,
    symbols_per_file: usize,
    json: bool,
    skeleton: bool,
) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (index_root, index_dir) = find_index_root(&path);

    let Some(index_dir) = index_dir else {
        eprintln!("No index found. Run 'og build' to create.");
        std::process::exit(EXIT_ERROR);
    };

    let manifest = match Manifest::load(&index_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(EXIT_ERROR);
        }
    };

    let vectors_path = index_dir.join(VECTORS_DIR).to_string_lossy().into_owned();
    let store = match omendb::VectorStore::open(&vectors_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open index: {e}");
            std::process::exit(EXIT_ERROR);
        }
    };

    let scope_prefix = path
        .strip_prefix(&index_root)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty());

    let mut block_ids: Vec<&String> = manifest
        .files
        .iter()
        .filter(|(rel_path, _)| in_scope(rel_path, scope_prefix.as_deref()))
        .flat_map(|(_, entry)| entry.blocks.iter())
        .collect();
    block_ids.sort();

    if block_ids.is_empty() {
        eprintln!("No indexed files under {}", path.display());
        std::process::exit(EXIT_ERROR);
    }

    let blocks = collect_blocks(&block_ids, &store);
    let ranked = rank_context(&blocks, num_files, symbols_per_file, skeleton);

    if json {
        println!("{}", serde_json::to_string_pretty(&ranked)?);
    } else {
        print_default(&ranked, skeleton);
    }

    Ok(())
}

fn in_scope(rel_path: &str, scope_prefix: Option<&str>) -> bool {
    match scope_prefix {
        Some(prefix) => rel_path == prefix || rel_path.starts_with(&format!("{prefix}/")),
        None => true,
    }
}

fn collect_blocks(block_ids: &[&String], store: &omendb::VectorStore) -> Vec<IndexedBlock> {
    let mut blocks: Vec<IndexedBlock> = block_ids
        .iter()
        .filter_map(|id| {
            let meta = store.get_metadata_by_id(id)?;
            let block_type = meta.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if DOC_BLOCK_TYPES.contains(&block_type) {
                return None;
            }

            Some(IndexedBlock {
                file: meta
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: meta
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                block_type: block_type.to_string(),
                start_line: meta.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                end_line: meta.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                content: meta
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                skeleton: meta
                    .get("skeleton")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    blocks.sort_by(|a, b| a.file.cmp(&b.file).then(a.start_line.cmp(&b.start_line)));
    blocks
}

fn rank_context(
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

fn print_default(files: &[RankedFile], include_skeleton: bool) {
    for file in files {
        println!(
            "{} {} {} {}",
            file.file.bold(),
            format!("score:{:.1}", file.score).dimmed(),
            format!("refs:{}", file.inbound_refs).dimmed(),
            format!("files:{}", file.inbound_files).dimmed(),
        );

        for symbol in &file.symbols {
            println!(
                "  {:>5}  {:<12}  {} {}",
                symbol.line,
                symbol.block_type.dimmed(),
                symbol.name,
                format!("score:{:.1}", symbol.score).dimmed(),
            );
            if include_skeleton {
                for line in symbol.skeleton.as_deref().unwrap_or("").lines() {
                    if !line.trim().is_empty() {
                        println!("         {}", line.dimmed());
                    }
                }
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::context::{IndexedBlock, rank_context};

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
