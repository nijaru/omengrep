pub mod build;
pub mod clean;
pub mod list;
pub mod mcp;
pub mod model;
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
    #[arg(long = "threshold", long = "min-score", default_value = "0.0")]
    threshold: f32,

    /// JSON output.
    #[arg(short = 'j', long = "json")]
    json: bool,

    /// List files only.
    #[arg(short = 'l', long = "files-only")]
    files_only: bool,

    /// Compact output (no content).
    #[arg(short = 'c', long = "compact")]
    compact: bool,

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
    /// Show embedding model status.
    Model {
        #[command(subcommand)]
        action: Option<ModelAction>,
    },
    /// Start MCP server (JSON-RPC over stdio).
    Mcp,
    /// Install og as MCP server in Claude Code.
    InstallClaudeCode,
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
        Some(Command::Model { action }) => match action {
            Some(ModelAction::Install) => model::install(),
            None => model::status(),
        },
        Some(Command::Mcp) => mcp::run(),
        Some(Command::InstallClaudeCode) => mcp::install_claude_code(),
        None => search::run(
            cli.query.as_deref(),
            &cli.path,
            cli.num_results,
            cli.threshold,
            cli.json,
            cli.files_only,
            cli.compact,
            cli.quiet,
            cli.file_types.as_deref(),
            &cli.exclude,
            cli.code_only,
            cli.no_index,
        ),
    }
}
