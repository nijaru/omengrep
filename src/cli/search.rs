use std::path::Path;

use anyhow::Result;

pub fn run(
    _query: Option<&str>,
    _path: &Path,
    _num_results: usize,
    _threshold: f32,
    _json: bool,
    _files_only: bool,
    _compact: bool,
    _quiet: bool,
    _file_types: Option<&str>,
    _exclude: &[String],
    _code_only: bool,
    _no_index: bool,
) -> Result<()> {
    todo!("search command")
}
