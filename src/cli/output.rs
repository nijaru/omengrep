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
    use owo_colors::OwoColorize;

    for r in results {
        let line_num = r.line.to_string();

        if show_score {
            let score_pct = ((r.score * 100.0) as i32).to_string();
            println!(
                "{}:{} {} {} ({}% similar)",
                r.file.cyan(),
                line_num.yellow(),
                r.block_type.dimmed(),
                r.name.bold(),
                score_pct.magenta()
            );
        } else {
            println!(
                "{}:{} {} {}",
                r.file.cyan(),
                line_num.yellow(),
                r.block_type.dimmed(),
                r.name.bold()
            );
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
