pub mod build;
pub mod clean;
pub mod context;
pub mod list;
pub mod model;
pub mod outline;
pub mod output;
pub mod search;
pub mod status;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "og", about = "Semantic code search", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Search query or file reference (file#name, file:line).
    #[arg(value_name = "QUERY")]
    query: Option<String>,

    /// Directory to search.
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,

    /// Number of results.
    #[arg(short = 'n', default_value = "10")]
    num_results: usize,

    /// Minimum similarity score (0 = disabled).
    #[arg(long = "threshold", default_value = "0.0")]
    threshold: f32,

    /// JSON output.
    #[arg(short = 'j', long = "json")]
    json: bool,

    /// List files only.
    #[arg(short = 'l', long = "files-only")]
    files_only: bool,

    /// JSON output without content field.
    #[arg(long = "no-content")]
    no_content: bool,

    /// Suppress progress.
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Filter file types (py,js,ts).
    #[arg(short = 't', long = "type")]
    file_types: Option<String>,

    /// Exclude glob patterns.
    #[arg(long = "exclude")]
    exclude: Vec<String>,

    /// Exclude docs (md, txt, rst).
    #[arg(long = "code-only")]
    code_only: bool,

    /// Skip auto-index (fail if missing).
    #[arg(long = "no-index")]
    no_index: bool,

    /// Content preview lines (0 = none).
    #[arg(short = 'C', long = "context", default_value = "5")]
    context_lines: usize,

    /// Filter results by regex (applied to content and name).
    #[arg(short = 'e', long = "regex")]
    regex: Option<String>,

    /// Highlight query-related tokens in terminal previews.
    #[arg(long = "highlight")]
    highlight: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Build or update index.
    Build {
        /// Directory to index.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Force full rebuild.
        #[arg(short = 'f', long = "force")]
        force: bool,
        /// Suppress progress.
        #[arg(short = 'q', long = "quiet")]
        quiet: bool,
    },
    /// Show index status.
    Status {
        /// Directory to check.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Delete index.
    Clean {
        /// Directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Also delete indexes in subdirectories.
        #[arg(short = 'r', long = "recursive")]
        recursive: bool,
    },
    /// List all indexes under a directory.
    List {
        /// Directory to search.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show block structure of an indexed file.
    Outline {
        /// File or directory to outline.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// JSON output.
        #[arg(short = 'j', long = "json")]
        json: bool,
        /// Output full function/class signatures without bodies.
        #[arg(long = "skeleton")]
        skeleton: bool,
    },
    /// Show ranked files and symbols for compact code context.
    Context {
        /// File or directory to summarize.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Number of files to show.
        #[arg(short = 'n', default_value = "12")]
        num_files: usize,
        /// Number of symbols per file.
        #[arg(long = "symbols", default_value = "5")]
        symbols_per_file: usize,
        /// JSON output.
        #[arg(short = 'j', long = "json")]
        json: bool,
        /// Include skeleton snippets.
        #[arg(long = "skeleton")]
        skeleton: bool,
    },
    /// Show embedding model status.
    Model {
        #[command(subcommand)]
        action: Option<ModelAction>,
    },
}

#[derive(Subcommand)]
enum ModelAction {
    /// Download embedding model.
    Install,
}

/// Main CLI entry point.
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Build { path, force, quiet }) => build::run(&path, force, quiet),
        Some(Command::Status { path }) => status::run(&path),
        Some(Command::Clean { path, recursive }) => clean::run(&path, recursive),
        Some(Command::List { path }) => list::run(&path),
        Some(Command::Outline {
            path,
            json,
            skeleton,
        }) => outline::run(&path, json, skeleton),
        Some(Command::Context {
            path,
            num_files,
            symbols_per_file,
            json,
            skeleton,
        }) => context::run(&path, num_files, symbols_per_file, json, skeleton),
        Some(Command::Model { action }) => match action {
            Some(ModelAction::Install) => model::install(),
            None => model::status(),
        },
        None if cli.query.is_none() => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
        None => search::run(&search::SearchParams {
            query: cli.query.as_deref(),
            path: &cli.path,
            num_results: cli.num_results,
            threshold: cli.threshold,
            format: crate::types::OutputFormat::from_flags(
                cli.json,
                cli.files_only,
                cli.no_content,
            ),
            quiet: cli.quiet,
            file_types: cli.file_types.as_deref(),
            exclude: &cli.exclude,
            code_only: cli.code_only,
            no_index: cli.no_index,
            context_lines: cli.context_lines,
            regex: cli.regex.as_deref(),
            highlight: cli.highlight,
        }),
    }
}
