//! og CLI: search, build, status, clean, outline, context, model.
//! usage-rs surface: one declaration drives parsing, help, dispatch,
//! and the portable spec (docs/completions via usage-cli).

pub mod build;
pub mod clean;
pub mod context;
pub mod model;
pub mod outline;
pub mod search;
pub mod status;

use std::path::PathBuf;

use usage::{Args, Cli, Run, Subcommands};

/// Semantic code search — hybrid BM25 + vectors
#[derive(Cli)]
#[usage(bin = "og", version)]
struct Og {
    /// Search query or file reference (file#name, file:line)
    query: Option<String>,

    /// Directory to search
    #[usage(default = ".")]
    path: PathBuf,

    /// Number of results
    #[usage(short = 'n', default = "10")]
    num_results: usize,

    /// JSON output
    #[usage(short = 'j', long)]
    json: bool,

    /// List files only
    #[usage(short = 'l', long)]
    files_only: bool,

    /// JSON output without content field
    #[usage(long)]
    no_content: bool,

    /// Suppress progress
    #[usage(short = 'q', long)]
    quiet: bool,

    /// Filter file types (py,js,ts)
    #[usage(short = 't', long, name = "type")]
    file_types: Option<String>,

    /// Exclude glob patterns
    #[usage(long)]
    exclude: Vec<String>,

    /// Exclude docs (md, txt, rst)
    #[usage(long)]
    code_only: bool,

    /// Disable the vector channel (BM25 + trigram only)
    #[usage(long)]
    no_semantic: bool,

    /// Content preview lines (0 = none)
    #[usage(short = 'C', long, name = "context", default = "5")]
    context_lines: usize,

    /// Filter results by regex (applied to content and name)
    #[usage(short = 'e', long)]
    regex: Option<String>,

    /// Highlight query-related tokens in terminal previews
    #[usage(long)]
    highlight: bool,

    /// Subcommand to run
    #[usage(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommands)]
#[usage(run)]
enum Command {
    /// Build or update index
    Build(Build),
    /// Show index status
    Status(Status),
    /// Delete index
    Clean(Clean),
    /// Show block structure of an indexed file
    Outline(Outline),
    /// Show ranked files and symbols for compact code context
    Context(Context),
    /// Embedding model management
    Model(Model),
}

#[derive(Args)]
struct Build {
    /// Directory to index
    #[usage(default = ".")]
    path: PathBuf,
    /// Use the deterministic test embedder (no model download; no semantic signal)
    #[usage(long)]
    deterministic: bool,
    /// Force a full rebuild
    #[usage(short = 'f', long)]
    force: bool,
    /// Suppress progress
    #[usage(short = 'q', long)]
    quiet: bool,
}

#[derive(Args)]
struct Status {
    /// Directory to check
    #[usage(default = ".")]
    path: PathBuf,
}

#[derive(Args)]
struct Clean {
    /// Directory
    #[usage(default = ".")]
    path: PathBuf,
    /// Suppress the removal confirmation
    #[usage(short = 'q', long)]
    quiet: bool,
}

#[derive(Args)]
struct Outline {
    /// File or directory to outline
    #[usage(default = ".")]
    path: PathBuf,
    /// JSON output
    #[usage(short = 'j', long)]
    json: bool,
    /// Output full function/class signatures without bodies
    #[usage(long)]
    skeleton: bool,
    /// Token budget for packed output (~4 chars/token)
    #[usage(long, default = "8000")]
    max_tokens: usize,
    /// Suppress progress
    #[usage(short = 'q', long)]
    quiet: bool,
}

#[derive(Args)]
struct Context {
    /// File or directory to summarize
    #[usage(default = ".")]
    path: PathBuf,
    /// Number of files to show
    #[usage(short = 'n', default = "12")]
    num_files: usize,
    /// Number of symbols per file
    #[usage(long, name = "symbols", default = "5")]
    symbols_per_file: usize,
    /// JSON output
    #[usage(short = 'j', long)]
    json: bool,
    /// Include skeleton snippets
    #[usage(long)]
    skeleton: bool,
    /// Token budget for packed output (~4 chars/token)
    #[usage(long, default = "8000")]
    max_tokens: usize,
    /// Suppress progress
    #[usage(short = 'q', long)]
    quiet: bool,
}

#[derive(Args)]
struct Model {
    #[usage(subcommand)]
    command: Option<ModelAction>,
}

#[derive(Subcommands)]
enum ModelAction {
    /// Show model status
    Status,
    /// Download the default model
    Install,
}

impl Run for Build {
    type Output = anyhow::Result<()>;
    fn run(self) -> Self::Output {
        build::run(&self.path, self.deterministic, self.force, self.quiet)
    }
}

impl Run for Status {
    type Output = anyhow::Result<()>;
    fn run(self) -> Self::Output {
        status::run(&self.path)
    }
}

impl Run for Clean {
    type Output = anyhow::Result<()>;
    fn run(self) -> Self::Output {
        clean::run(&self.path, self.quiet)
    }
}

impl Run for Outline {
    type Output = anyhow::Result<()>;
    fn run(self) -> Self::Output {
        outline::run(&outline::OutlineParams {
            path: &self.path,
            json: self.json,
            skeleton: self.skeleton,
            max_tokens: self.max_tokens,
            quiet: self.quiet,
        })
    }
}

impl Run for Context {
    type Output = anyhow::Result<()>;
    fn run(self) -> Self::Output {
        context::run(&context::ContextParams {
            path: &self.path,
            num_files: self.num_files,
            symbols_per_file: self.symbols_per_file,
            json: self.json,
            skeleton: self.skeleton,
            max_tokens: self.max_tokens,
            quiet: self.quiet,
        })
    }
}

impl Run for Model {
    type Output = anyhow::Result<()>;
    fn run(self) -> Self::Output {
        match self.command {
            Some(ModelAction::Status) => model::status(),
            Some(ModelAction::Install) => model::install(),
            None => model::status(),
        }
    }
}

/// Main CLI entry point.
pub fn run() -> anyhow::Result<()> {
    let cli = Og::parse();

    match cli.command {
        Some(command) => command.run(),
        // No subcommand: `og "query" [path]` is the default search
        // surface; a completely bare `og` prints help.
        None => match cli.query {
            Some(query) => search::run(&search::SearchParams {
                query: Some(query.as_str()),
                path: &cli.path,
                num_results: cli.num_results,
                format: og_core::types::OutputFormat::from_flags(
                    cli.json,
                    cli.files_only,
                    cli.no_content,
                ),
                quiet: cli.quiet,
                file_types: cli.file_types.as_deref(),
                exclude: &cli.exclude,
                code_only: cli.code_only,
                no_semantic: cli.no_semantic,
                context_lines: cli.context_lines,
                regex: cli.regex.as_deref(),
                highlight: cli.highlight,
            }),
            None => {
                let page = Og::render_help(Og::command(), false).unwrap_or_default();
                print!("{page}");
                Ok(())
            }
        },
    }
}
