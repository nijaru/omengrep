use std::path::Path;

use crate::types::{OutputFormat, SearchResult};

/// Print search results in the specified format.
pub fn print_results(
    results: &[SearchResult],
    format: OutputFormat,
    show_score: bool,
    root: Option<&Path>,
) {
    let results: Vec<SearchResult> = results
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if let Some(root) = root {
                if let Ok(rel) = Path::new(&r.file).strip_prefix(root) {
                    r.file = rel.to_string_lossy().into_owned();
                }
            }
            r
        })
        .collect();

    match format {
        OutputFormat::FilesOnly => print_files_only(&results),
        OutputFormat::Json => print_json(&results, false),
        OutputFormat::Compact => print_json(&results, true),
        OutputFormat::Default => print_default(&results, show_score),
    }
}

fn print_files_only(results: &[SearchResult]) {
    let mut seen = std::collections::HashSet::new();
    for r in results {
        if seen.insert(&r.file) {
            println!("{}", r.file);
        }
    }
}

fn print_json(results: &[SearchResult], compact: bool) {
    if compact {
        let output: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let mut v = serde_json::to_value(r).unwrap_or_default();
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("content");
                }
                v
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(results).unwrap_or_default()
        );
    }
}

fn print_default(results: &[SearchResult], show_score: bool) {
    use colored::Colorize;

    for r in results {
        let type_str = r.block_type.dimmed();
        let name_str = r.name.bold();
        let file_str = r.file.cyan();
        let line_str = r.line.to_string().yellow();

        if show_score {
            let score_pct = (r.score * 100.0) as i32;
            println!(
                "{file_str}:{line_str} {type_str} {name_str} ({}% similar)",
                score_pct.to_string().magenta()
            );
        } else {
            println!("{file_str}:{line_str} {type_str} {name_str}");
        }

        // Content preview (first 3 non-empty lines)
        if let Some(content) = &r.content {
            let preview_lines: Vec<&str> = content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(3)
                .collect();
            for line in preview_lines {
                let truncated = if line.len() > 80 {
                    format!("{}...", &line[..77])
                } else {
                    line.to_string()
                };
                println!("  {}", truncated.dimmed());
            }
            println!();
        }
    }
}
