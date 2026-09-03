//! File block-structure outline. Ported from og v0.0.3 cli/outline.rs:
//! same scope filtering, path-then-line ordering, default + JSON shapes.
//! Block rows come from the SQLite catalog instead of omendb metadata.

use owo_colors::OwoColorize;
use serde::Serialize;

use super::BlockRow;

/// One block in an outline listing (1-based lines, matching v0.0.3 output).
#[derive(Clone, Serialize)]
pub struct OutlineEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<String>,
}

/// One file's outline: path + blocks in source order.
#[derive(Serialize)]
pub struct OutlineFile {
    pub file: String,
    pub blocks: Vec<OutlineEntry>,
}

/// Assemble outlines for `files` (sorted paths) from catalog `rows`
/// (ordered by file, start_line). `include_skeleton` attaches signature
/// snippets; `max_tokens` caps packed output (see `super::pack` policy).
pub fn build_outline(
    files: &[String],
    rows: &[BlockRow],
    include_skeleton: bool,
    max_tokens: usize,
) -> Vec<OutlineFile> {
    let mut by_file: std::collections::HashMap<&str, Vec<&BlockRow>> = Default::default();
    for row in rows {
        by_file.entry(row.file.as_str()).or_default().push(row);
    }

    let mut out: Vec<OutlineFile> = Vec::with_capacity(files.len());
    let mut budget = super::TokenBudget::new(max_tokens);
    for file in files {
        let mut entries: Vec<OutlineEntry> = Vec::new();
        if let Some(blocks) = by_file.get(file.as_str()) {
            for row in blocks {
                let skeleton = include_skeleton.then(|| row.skeleton.clone());
                let cost = super::estimate_tokens(&row.name)
                    + super::estimate_tokens(&row.block_type)
                    + 2
                    + skeleton.as_deref().map(super::estimate_tokens).unwrap_or(0);
                if !budget.take(cost, !out.is_empty() || !entries.is_empty()) {
                    continue;
                }
                entries.push(OutlineEntry {
                    name: row.name.clone(),
                    block_type: row.block_type.clone(),
                    line: row.start_line + 1,
                    end_line: row.end_line + 1,
                    skeleton,
                });
            }
        }
        let header_cost = super::estimate_tokens(file) + 1;
        if entries.is_empty() && !budget.take(header_cost, !out.is_empty()) {
            continue;
        }
        // Header cost only matters when the file contributes nothing; a
        // file with blocks already paid per-block costs above.
        out.push(OutlineFile {
            file: file.clone(),
            blocks: entries,
        });
    }
    out
}

/// Legacy JSON shape: `[{file, blocks: [{name, type, line, end_line,
/// skeleton?}]}]`.
pub fn outline_json(files: &[OutlineFile]) -> serde_json::Value {
    serde_json::to_value(files).unwrap_or(serde_json::Value::Null)
}

pub fn print_default(files: &[OutlineFile]) {
    for file in files {
        println!("{}", file.file.bold());
        for entry in &file.blocks {
            println!(
                "  {:>5}  {:<12}  {}",
                entry.line,
                entry.block_type.dimmed(),
                entry.name
            );
            if let Some(skeleton) = &entry.skeleton {
                for line in skeleton.lines() {
                    println!("         {}", line.dimmed());
                }
            }
        }
        println!();
    }
}
