use std::path::Path;

use crate::types::{OutputFormat, SearchResult};

/// Print search results in the specified format.
pub fn print_results(
    results: &[SearchResult],
    format: OutputFormat,
    show_score: bool,
    root: Option<&Path>,
    context_lines: usize,
    highlight_query: Option<&str>,
) {
    let results: Vec<SearchResult> = results
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if let Some(root) = root
                && let Ok(rel) = Path::new(&r.file).strip_prefix(root)
            {
                r.file = rel.to_string_lossy().into_owned();
            }
            r
        })
        .collect();

    match format {
        OutputFormat::FilesOnly => print_files_only(&results),
        OutputFormat::Json => print_json(&results, false),
        OutputFormat::NoContent => print_json(&results, true),
        OutputFormat::Default => {
            print_default(&results, show_score, context_lines, highlight_query)
        }
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

fn print_default(
    results: &[SearchResult],
    show_score: bool,
    context_lines: usize,
    highlight_query: Option<&str>,
) {
    use owo_colors::OwoColorize;

    let highlight_terms = highlight_query.map(query_highlight_terms);

    for r in results {
        let line_num = r.line.to_string();

        if show_score {
            println!(
                "{}:{} {} {} (score: {:.3})",
                r.file.cyan(),
                line_num.yellow(),
                r.block_type.dimmed(),
                r.name.bold(),
                r.score
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

        if context_lines > 0
            && let Some(content) = &r.content
        {
            let preview_lines: Vec<&str> = content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(context_lines)
                .collect();
            for line in preview_lines {
                if let Some(terms) = &highlight_terms {
                    println!("  {}", highlight_line(line, terms));
                } else {
                    println!("  {}", line.dimmed());
                }
            }
            println!();
        }
    }
}

fn query_highlight_terms(query: &str) -> std::collections::HashSet<String> {
    let split = crate::tokenize::split_identifiers(query);
    let expanded = crate::synonyms::expand_query(&split);
    crate::tokenize::extract_terms(&expanded)
        .into_iter()
        .filter(|term| term.len() >= 3)
        .collect()
}

fn highlight_line(line: &str, terms: &std::collections::HashSet<String>) -> String {
    let mut output = String::new();
    let mut token_start = None;
    let mut segment_start = 0;

    for (idx, ch) in line.char_indices() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            token_start.get_or_insert(idx);
            continue;
        }

        if let Some(start) = token_start.take() {
            push_dimmed(&mut output, &line[segment_start..start]);
            push_highlighted_token(&mut output, &line[start..idx], terms);
            segment_start = idx;
        }
    }

    if let Some(start) = token_start {
        push_dimmed(&mut output, &line[segment_start..start]);
        push_highlighted_token(&mut output, &line[start..], terms);
    } else {
        push_dimmed(&mut output, &line[segment_start..]);
    }

    output
}

fn push_dimmed(output: &mut String, text: &str) {
    use owo_colors::OwoColorize;

    if !text.is_empty() {
        output.push_str(&text.dimmed().to_string());
    }
}

fn push_highlighted_token(
    output: &mut String,
    token: &str,
    terms: &std::collections::HashSet<String>,
) {
    use owo_colors::OwoColorize;

    let token_terms = crate::tokenize::extract_terms(token);
    if token_terms.iter().any(|term| terms.contains(term)) {
        output.push_str(&token.yellow().bold().to_string());
    } else {
        output.push_str(&token.dimmed().to_string());
    }
}
