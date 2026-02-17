use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use crate::index::{self, walker, SemanticIndex};
use crate::types::EXIT_ERROR;

pub fn run(path: &Path, force: bool, quiet: bool) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Check for parent index that already covers this path
    let build_path = if !index_exists(&path) {
        if let Some(parent) = index::find_parent_index(&path) {
            if !force {
                if !quiet {
                    eprintln!("Using parent index at {}", parent.display());
                }
                parent
            } else {
                path.clone()
            }
        } else {
            path.clone()
        }
    } else {
        path.clone()
    };

    // Find subdir indexes that will be superseded
    let subdir_indexes = index::find_subdir_indexes(&build_path, false);

    if force {
        // Full rebuild: always clear index dir (handles corrupt/partial state)
        let index_dir = build_path.join(crate::index::INDEX_DIR);
        if index_dir.exists() {
            std::fs::remove_dir_all(&index_dir)?;
        }
        build_index(&build_path, quiet)?;
    } else if index_exists(&build_path) {
        // Incremental update
        if !quiet {
            eprint!("Scanning files...");
        }
        let files = walker::scan(&build_path)?;
        if !quiet {
            eprintln!("\r                 \r");
        }

        let index = SemanticIndex::new(&build_path, None)?;
        let stale_result = index.get_stale_files(&files);

        match stale_result {
            Ok((changed, deleted)) => {
                let stale_count = changed.len() + deleted.len();
                if stale_count == 0 {
                    if !quiet {
                        eprintln!("Index up to date");
                    }
                } else {
                    if !quiet {
                        eprint!("Updating {stale_count} files...");
                    }
                    let stats = index.update(&files)?;
                    if !quiet {
                        eprintln!(
                            "\rUpdated {} blocks from {} files        ",
                            stats.blocks, stats.files
                        );
                        if stats.deleted > 0 {
                            eprintln!("  Removed {} stale blocks", stats.deleted);
                        }
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("older version") || msg.contains("different model") {
                    // Model or format changed - force rebuild
                    if !quiet {
                        eprintln!("Rebuilding (index format changed)...");
                    }
                    let idx = SemanticIndex::new(&build_path, None)?;
                    idx.clear()?;
                    build_index(&build_path, quiet)?;
                } else {
                    eprintln!("{e}");
                    std::process::exit(EXIT_ERROR);
                }
            }
        }
    } else {
        // No index exists - merge subdir indexes, then build
        if !subdir_indexes.is_empty() {
            // TODO: implement merge_from_subdir for VectorStore
            // For now, just build fresh and clean up subdirs after
        }

        build_index(&build_path, quiet)?;

        // Clean up subdir indexes (now superseded by parent)
        for idx in &subdir_indexes {
            let _ = std::fs::remove_dir_all(idx);
        }
        if !subdir_indexes.is_empty() && !quiet {
            eprintln!("Cleaned up {} subdir indexes", subdir_indexes.len());
        }
    }

    // Clean up subdir indexes for non-fresh builds
    if index_exists(&build_path) {
        for idx in &subdir_indexes {
            let _ = std::fs::remove_dir_all(idx);
        }
        if !subdir_indexes.is_empty() && !quiet {
            eprintln!("Cleaned up {} subdir indexes", subdir_indexes.len());
        }
    }

    Ok(())
}

fn index_exists(path: &Path) -> bool {
    path.join(crate::index::INDEX_DIR)
        .join("manifest.json")
        .exists()
}

fn build_index(path: &Path, quiet: bool) -> Result<()> {
    if !quiet {
        eprint!("Scanning files...");
    }
    let files = walker::scan(path)?;
    if !quiet {
        eprintln!("\r                 \r");
    }

    if files.is_empty() {
        if !quiet {
            eprintln!("No files found to index");
        }
        return Ok(());
    }

    let index = SemanticIndex::new(path, None)?;
    let t0 = Instant::now();

    let progress_fn = if quiet {
        None
    } else {
        Some(
            (|current: usize, total: usize, _msg: &str| {
                eprint!("\rIndexing {current}/{total}...");
            }) as fn(usize, usize, &str),
        )
    };

    let stats = index.index(
        &files,
        progress_fn
            .as_ref()
            .map(|f| f as &dyn Fn(usize, usize, &str)),
    )?;
    let elapsed = t0.elapsed();

    if !quiet {
        eprintln!(
            "\rIndexed {} blocks from {} files ({:.1}s)        ",
            stats.blocks,
            stats.files,
            elapsed.as_secs_f64()
        );
        if stats.errors > 0 {
            eprintln!("{} files failed to index", stats.errors);
        }
    }

    Ok(())
}
