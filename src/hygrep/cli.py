"""hygrep CLI - Hybrid grep with neural reranking."""

import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Annotated, Optional

import typer
from rich.console import Console
from rich.panel import Panel
from rich.progress import BarColumn, Progress, TextColumn
from rich.syntax import Syntax
from rich.table import Table

try:
    import tomllib
except ImportError:
    import tomli as tomllib

import contextlib

import pathspec

from . import __version__
from .reranker import (
    MODEL_REPO,
    clean_model_cache,
    download_model,
    get_execution_providers,
    get_model_info,
)

# Consoles for rich output
console = Console()
err_console = Console(stderr=True)

# Config file location
CONFIG_PATH = Path.home() / ".config" / "hygrep" / "config.toml"

# File extension to Pygments lexer mapping for syntax highlighting
EXT_TO_LEXER: dict[str, str] = {
    ".py": "python",
    ".pyi": "python",
    ".js": "javascript",
    ".jsx": "jsx",
    ".ts": "typescript",
    ".tsx": "tsx",
    ".rs": "rust",
    ".go": "go",
    ".java": "java",
    ".c": "c",
    ".h": "c",
    ".cpp": "cpp",
    ".cc": "cpp",
    ".cxx": "cpp",
    ".hpp": "cpp",
    ".hh": "cpp",
    ".cs": "csharp",
    ".rb": "ruby",
    ".php": "php",
    ".sh": "bash",
    ".bash": "bash",
    ".zsh": "zsh",
    ".fish": "fish",
    ".md": "markdown",
    ".json": "json",
    ".yaml": "yaml",
    ".yml": "yaml",
    ".toml": "toml",
    ".html": "html",
    ".css": "css",
    ".scss": "scss",
    ".sql": "sql",
    ".lua": "lua",
    ".vim": "vim",
    ".swift": "swift",
    ".kt": "kotlin",
    ".scala": "scala",
    ".r": "r",
    ".R": "r",
    ".mojo": "python",  # Closest approximation
    ".🔥": "python",
}


def load_config() -> dict:
    """Load config from ~/.config/hygrep/config.toml if it exists."""
    if not CONFIG_PATH.exists():
        return {}
    try:
        with open(CONFIG_PATH, "rb") as f:
            return tomllib.load(f)
    except Exception as e:
        err_console.print(f"[yellow]Warning:[/] Failed to load config: {e}")
        return {}


# Exit codes (grep convention)
EXIT_MATCH = 0
EXIT_NO_MATCH = 1
EXIT_ERROR = 2

# Examples panel for help
EXAMPLES = """
[bold]Examples:[/]
  [cyan]hhg[/] "auth" ./src              Search with neural reranking
  [cyan]hhg[/] "error" ./src --fast      Fast grep (no model, instant)
  [cyan]hhg[/] "TODO" . -t py,rs         Filter by file type
  [cyan]hhg[/] "api" . --json -n 20      JSON output, 20 results
  [cyan]hhg model install[/]             Download the reranking model
  [cyan]hhg info[/]                      Show installation status
"""

# Create the Typer app
app = typer.Typer(
    name="hhg",
    help="[bold]Hybrid grep:[/] fast scanning + neural reranking\n\n" + EXAMPLES,
    no_args_is_help=False,  # We handle empty args in callback
    invoke_without_command=True,
    rich_markup_mode="rich",
    add_completion=False,
    context_settings={
        "help_option_names": ["-h", "--help"],
        "allow_interspersed_args": True,
    },
)

# Model subcommand group
model_app = typer.Typer(
    help="Manage the reranking model",
    no_args_is_help=True,
    rich_markup_mode="rich",
)
app.add_typer(model_app, name="model")


def version_callback(value: bool):
    if value:
        console.print(f"hygrep {__version__}")
        raise typer.Exit()


def completions_callback(shell: Optional[str]):
    if shell:
        if shell == "bash":
            print(BASH_COMPLETION.strip())
        elif shell == "zsh":
            print(ZSH_COMPLETION.strip())
        elif shell == "fish":
            print(FISH_COMPLETION.strip())
        raise typer.Exit()


# ============================================================================
# Main Search (default action via callback)
# ============================================================================


@app.callback()
def search(
    ctx: typer.Context,
    query: Annotated[
        Optional[str], typer.Argument(help="Search query (natural language or regex)")
    ] = None,
    path: Annotated[Path, typer.Argument(help="Directory to search")] = Path("."),
    # Output options
    n: Annotated[int, typer.Option("-n", help="Number of results")] = 10,
    json_output: Annotated[bool, typer.Option("--json", help="Output JSON for scripts")] = False,
    quiet: Annotated[bool, typer.Option("-q", "--quiet", help="Suppress progress")] = False,
    context: Annotated[int, typer.Option("-C", "--context", help="Lines of context")] = 0,
    color: Annotated[str, typer.Option(help="Color: auto/always/never")] = "auto",
    stats: Annotated[bool, typer.Option("--stats", help="Show timing stats")] = False,
    # Filtering options
    file_types: Annotated[
        Optional[str], typer.Option("-t", "--type", help="Filter types (py,js,ts...)")
    ] = None,
    exclude: Annotated[Optional[list[str]], typer.Option("--exclude", help="Exclude glob")] = None,
    min_score: Annotated[float, typer.Option("--min-score", help="Min score threshold")] = 0.0,
    no_ignore: Annotated[bool, typer.Option("--no-ignore", help="Ignore .gitignore")] = False,
    hidden: Annotated[bool, typer.Option("--hidden", help="Include hidden files")] = False,
    # Performance options
    fast: Annotated[bool, typer.Option("--fast", help="Skip reranking (instant)")] = False,
    max_candidates: Annotated[int, typer.Option("--max-candidates", help="Max to rerank")] = 100,
    # Meta options
    version: Annotated[
        Optional[bool],
        typer.Option("-v", "--version", callback=version_callback, is_eager=True, help="Version"),
    ] = None,
    completions: Annotated[
        Optional[str],
        typer.Option(
            "--completions", callback=completions_callback, is_eager=True, help="Shell completions"
        ),
    ] = None,
    # Hidden option for model install --force (parsed here, used in model subcommand handling)
    force: Annotated[bool, typer.Option("--force", "-f", hidden=True)] = False,
):
    """Search code with natural language or regex."""
    # Skip if a subcommand was invoked
    if ctx.invoked_subcommand is not None:
        return

    # Check if query is actually a subcommand name (Typer parses positional args first)
    if query == "info":
        # Invoke info command directly
        info()
        raise typer.Exit()
    if query == "model":
        # Handle model subcommands (path contains the subcommand due to arg parsing)
        subcmd = str(path)
        if subcmd == "install":
            model_install(force=force)
        elif subcmd == "clean":
            model_clean()
        elif subcmd == "status":
            model_status()
        else:
            err_console.print("[yellow]Usage:[/] hhg model [install|clean|status]")
            err_console.print("Run [cyan]hhg model --help[/] for more info")
        raise typer.Exit()

    # No query = show help panel
    if not query:
        console.print(
            Panel(EXAMPLES.strip(), title="[bold]hhg[/] - Hybrid Grep", border_style="dim")
        )
        raise typer.Exit()

    # Load config and apply defaults
    config = load_config()
    if config:
        if n == 10 and "n" in config:
            n = config["n"]
        if max_candidates == 100 and "max_candidates" in config:
            max_candidates = config["max_candidates"]
        if color == "auto" and "color" in config:
            color = config["color"]
        if min_score == 0.0 and "min_score" in config:
            min_score = config["min_score"]
        if not fast and config.get("fast", False):
            fast = True
        if not quiet and config.get("quiet", False):
            quiet = True
        if not hidden and config.get("hidden", False):
            hidden = True
        if not no_ignore and config.get("no_ignore", False):
            no_ignore = True
        if not exclude and "exclude" in config:
            exc = config["exclude"]
            exclude = exc if isinstance(exc, list) else [exc]

    # Determine color usage
    use_color = (color == "always") or (color == "auto" and sys.stdout.isatty() and not json_output)
    # Respect NO_COLOR standard
    if os.environ.get("NO_COLOR"):
        use_color = False

    # Validate path
    if not path.exists():
        if json_output:
            print(json.dumps({"error": f"Path does not exist: {path}"}))
        else:
            err_console.print(f"[red]Error:[/] Path does not exist: {path}")
        raise typer.Exit(EXIT_ERROR)

    if not path.is_dir():
        if json_output:
            print(json.dumps({"error": f"Path is not a directory: {path}"}))
        else:
            err_console.print(f"[red]Error:[/] Path is not a directory: {path}")
        raise typer.Exit(EXIT_ERROR)

    # Stats tracking
    stats_data = {"scan_ms": 0, "filter_ms": 0, "rerank_ms": 0, "total_ms": 0}
    total_start = time.perf_counter()

    # Query expansion: "login auth" -> "login|auth"
    scanner_query = query
    if " " in query and not _is_regex_pattern(query):
        words = query.split()
        escaped_words = [re.escape(w) for w in words]
        scanner_query = "|".join(escaped_words)

    # 1. Recall phase - Try Mojo scanner, fall back to Python
    try:
        from ._scanner import scan
    except ImportError:
        from .scanner import scan

    # Scan with spinner
    if not quiet and not json_output:
        with err_console.status(f"[bold blue]Scanning[/] {path}...", spinner="dots"):
            scan_start = time.perf_counter()
            file_contents = scan(str(path), scanner_query, hidden)
            stats_data["scan_ms"] = int((time.perf_counter() - scan_start) * 1000)
    else:
        scan_start = time.perf_counter()
        file_contents = scan(str(path), scanner_query, hidden)
        stats_data["scan_ms"] = int((time.perf_counter() - scan_start) * 1000)

    filter_start = time.perf_counter()

    # Filter by gitignore
    if not no_ignore:
        gitignore_spec = _load_gitignore(path)
        if gitignore_spec:
            file_contents = {
                k: v
                for k, v in file_contents.items()
                if not gitignore_spec.match_file(k.lstrip("./"))
            }

    # Filter by exclude patterns
    if exclude:
        exclude_spec = pathspec.PathSpec.from_lines("gitwildmatch", exclude)
        file_contents = {
            k: v for k, v in file_contents.items() if not exclude_spec.match_file(k.lstrip("./"))
        }

    # Filter by file type
    if file_types:
        type_map = {
            "py": [".py"],
            "js": [".js", ".jsx"],
            "ts": [".ts", ".tsx"],
            "rust": [".rs"],
            "rs": [".rs"],
            "go": [".go"],
            "mojo": [".mojo", ".🔥"],
            "java": [".java"],
            "c": [".c", ".h"],
            "cpp": [".cpp", ".cc", ".cxx", ".hpp", ".hh"],
            "cs": [".cs"],
            "rb": [".rb"],
            "php": [".php"],
            "sh": [".sh", ".bash"],
            "md": [".md", ".markdown"],
            "json": [".json"],
            "yaml": [".yaml", ".yml"],
            "toml": [".toml"],
        }
        allowed_exts = set()
        for file_type in file_types.split(","):
            ft = file_type.strip().lower()
            if ft in type_map:
                allowed_exts.update(type_map[ft])
            else:
                allowed_exts.add(f".{ft}")
        file_contents = {
            k: v for k, v in file_contents.items() if any(k.endswith(ext) for ext in allowed_exts)
        }

    stats_data["filter_ms"] = int((time.perf_counter() - filter_start) * 1000)

    if not quiet and not json_output:
        err_console.print(f"[dim]Found {len(file_contents)} candidates[/]")

    if len(file_contents) == 0:
        if json_output:
            print("[]")
        raise typer.Exit(EXIT_NO_MATCH)

    # 2. Rerank phase (or fast mode)
    rerank_start = time.perf_counter()
    reranker = None

    if fast:
        from .extractor import ContextExtractor

        extractor = ContextExtractor()
        results = []
        for filepath, content in list(file_contents.items())[:n]:
            blocks = extractor.extract(filepath, query, content=content)
            results.extend(
                {
                    "file": filepath,
                    "type": block["type"],
                    "name": block["name"],
                    "start_line": block["start_line"],
                    "content": block["content"],
                    "score": 0.0,
                }
                for block in blocks
            )
        results = results[:n]
    else:
        from .reranker import Reranker

        reranker = Reranker()

        # Rerank with progress bar
        if not quiet and not json_output:
            with Progress(
                TextColumn("[bold blue]Reranking"),
                BarColumn(),
                TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
                console=err_console,
                transient=True,
            ) as progress:
                task = progress.add_task("rerank", total=100)

                def update_progress(current: int, total: int) -> None:
                    progress.update(task, completed=int(current / total * 100))

                results = reranker.search(
                    query,
                    file_contents,
                    top_k=n,
                    max_candidates=max_candidates,
                    progress_callback=update_progress,
                )
        else:
            results = reranker.search(query, file_contents, top_k=n, max_candidates=max_candidates)

    stats_data["rerank_ms"] = int((time.perf_counter() - rerank_start) * 1000)
    stats_data["total_ms"] = int((time.perf_counter() - total_start) * 1000)

    # Filter by min-score
    if min_score > 0 and not fast:
        results = [r for r in results if r["score"] >= min_score]

    # Output results
    if json_output:
        print(json.dumps(results))
        raise typer.Exit(EXIT_MATCH if results else EXIT_NO_MATCH)

    if not results:
        err_console.print("[dim]No relevant results.[/]")
        raise typer.Exit(EXIT_NO_MATCH)

    # Print results
    for item in results:
        _print_result(item, fast, use_color, context)

    # Show stats
    if stats:
        _print_stats(stats_data, reranker, use_color)

    raise typer.Exit(EXIT_MATCH)


# ============================================================================
# Info Command
# ============================================================================


@app.command()
def info():
    """Show installation status and system info."""
    console.print(f"\n[bold]hygrep[/] {__version__}\n")

    table = Table(show_header=False, box=None, padding=(0, 2))
    table.add_column("Status", style="bold")
    table.add_column("Component")
    table.add_column("Details", style="dim")

    # Model status
    model_info = get_model_info()
    if model_info["installed"]:
        table.add_row("[green]✓[/]", "Model", f"Installed ({model_info['size_mb']:.0f}MB)")
    else:
        table.add_row("[yellow]○[/]", "Model", "Not installed → [cyan]hhg model install[/]")

    # Device/provider
    providers = get_execution_providers()
    provider = providers[0].replace("ExecutionProvider", "")
    table.add_row("[green]✓[/]", "Device", provider)

    # Scanner
    try:
        from ._scanner import scan  # noqa: F401

        table.add_row("[green]✓[/]", "Scanner", "Mojo native")
    except ImportError:
        table.add_row("[green]✓[/]", "Scanner", "Python fallback")

    # Languages
    try:
        from .extractor import LANGUAGE_CAPSULES

        count = len(set(ext.lstrip(".") for ext in LANGUAGE_CAPSULES if not ext.startswith(".🔥")))
        table.add_row("[green]✓[/]", "Languages", f"{count} supported")
    except ImportError:
        table.add_row("[red]✗[/]", "Languages", "Error loading")

    console.print(table)
    console.print()

    # Additional info
    if CONFIG_PATH.exists():
        console.print(f"[dim]Config:[/]  {CONFIG_PATH}")
    console.print(f"[dim]Repo:[/]    {MODEL_REPO}")
    if model_info["cache_dir"]:
        console.print(f"[dim]Cache:[/]   {model_info['cache_dir']}")
    else:
        console.print("[dim]Cache:[/]   ~/.cache/huggingface")
    console.print()


# ============================================================================
# Model Commands
# ============================================================================


@model_app.command("install")
def model_install(
    force: Annotated[bool, typer.Option("--force", "-f", help="Force re-download")] = False,
):
    """Download the reranking model from HuggingFace."""
    try:
        with err_console.status("[bold blue]Downloading model...", spinner="dots"):
            download_model(force=force, quiet=True)
        console.print("[green]✓[/] Model installed successfully")
    except Exception as e:
        err_console.print(f"[red]Error:[/] {e}")
        raise typer.Exit(EXIT_ERROR)


@model_app.command("clean")
def model_clean():
    """Remove cached model files."""
    if clean_model_cache():
        console.print("[green]✓[/] Model cache cleaned")
    else:
        console.print("[yellow]No cached model to clean[/]")
        raise typer.Exit(EXIT_NO_MATCH)


@model_app.command("status")
def model_status():
    """Show model installation status."""
    info = get_model_info()

    table = Table(show_header=False, box=None)
    table.add_column("Key", style="dim")
    table.add_column("Value")

    table.add_row("Repository", info["repo"])
    table.add_row(
        "Status", "[green]Installed[/]" if info["installed"] else "[yellow]Not installed[/]"
    )

    if info["installed"]:
        table.add_row("Size", f"{info['size_mb']:.0f}MB")
        table.add_row("Path", str(info["model_path"]))

    if info["cache_dir"]:
        table.add_row("Cache", str(info["cache_dir"]))
    else:
        table.add_row("Cache", "~/.cache/huggingface")

    console.print(table)


# ============================================================================
# Helper Functions
# ============================================================================


def _is_regex_pattern(query: str) -> bool:
    """Check if query contains regex metacharacters."""
    return any(c in query for c in r"*()[]\\|+?^$.{}")


def _load_gitignore(root: Path) -> pathspec.PathSpec | None:
    """Load .gitignore patterns from root directory and parents."""
    patterns = []
    current = root.resolve()

    while current != current.parent:
        gitignore = current / ".gitignore"
        if gitignore.exists():
            with contextlib.suppress(OSError, UnicodeDecodeError):
                patterns.extend(gitignore.read_text().splitlines())
        if (current / ".git").exists():
            break
        current = current.parent

    if not patterns:
        return None
    return pathspec.PathSpec.from_lines("gitwildmatch", patterns)


def _get_lexer_for_file(filepath: str) -> str | None:
    """Get Pygments lexer name for a file based on extension."""
    ext = Path(filepath).suffix.lower()
    # pathlib.suffix returns empty string for multi-byte emoji extensions like .🔥
    if not ext and filepath.endswith(".🔥"):
        ext = ".🔥"
    return EXT_TO_LEXER.get(ext)


def _print_result(item: dict, fast: bool, use_color: bool, context: int):
    """Print a single search result."""
    file = item["file"]
    name = item["name"]
    score = item["score"]
    kind = item["type"]
    start_line = item["start_line"]
    content = item.get("content", "")

    if use_color:
        path_str = f"[cyan]{file}[/]:[green]{start_line}[/]"
        type_str = f"[yellow]\\[{kind}][/]"
        name_str = f"[bold]{name}[/]"
        if fast:
            console.print(f"{path_str} {type_str} {name_str}")
        else:
            score_str = f"[magenta]({score:.2f})[/]"
            console.print(f"{path_str} {type_str} {name_str} {score_str}")
    else:
        if fast:
            print(f"{file}:{start_line} [{kind}] {name}")
        else:
            print(f"{file}:{start_line} [{kind}] {name} ({score:.2f})")

    # Show context with syntax highlighting
    if context > 0 and content:
        lines = content.splitlines()
        context_lines = lines[:context]
        context_text = "\n".join(context_lines)

        if use_color:
            # Infer language from file extension
            lexer = _get_lexer_for_file(file)
            if lexer:
                syntax = Syntax(
                    context_text,
                    lexer,
                    line_numbers=True,
                    start_line=start_line,
                    indent_guides=False,
                    word_wrap=False,
                )
                console.print(syntax)
            else:
                # Fallback to plain text with line numbers
                for i, line in enumerate(context_lines):
                    line_num = start_line + i
                    console.print(f"  [green]{line_num:4d}[/] │ {line}")
        else:
            for i, line in enumerate(context_lines):
                line_num = start_line + i
                print(f"  {line_num:4d} │ {line}")

        if len(lines) > context:
            remaining = len(lines) - context
            if use_color:
                console.print(f"  [dim]... ({remaining} more lines)[/]")
            else:
                print(f"       ... ({remaining} more lines)")
        print()


def _print_stats(stats: dict, reranker, use_color: bool):
    """Print timing statistics."""
    if use_color:
        console.print()
        table = Table(title="[bold]Stats[/]", show_header=False, box=None)
        table.add_column("Metric", style="dim")
        table.add_column("Time", justify="right")
        table.add_row("Scan", f"{stats['scan_ms']}ms")
        table.add_row("Filter", f"{stats['filter_ms']}ms")
        table.add_row("Rerank", f"{stats['rerank_ms']}ms")
        table.add_row("Total", f"[bold]{stats['total_ms']}ms[/]")
        if reranker:
            provider = reranker.provider.replace("ExecutionProvider", "")
            table.add_row("Device", provider)
        console.print(table)
    else:
        print("\nStats:")
        print(f"  Scan:   {stats['scan_ms']:>5}ms")
        print(f"  Filter: {stats['filter_ms']:>5}ms")
        print(f"  Rerank: {stats['rerank_ms']:>5}ms")
        print(f"  Total:  {stats['total_ms']:>5}ms")


# ============================================================================
# Shell Completions
# ============================================================================

BASH_COMPLETION = """
_hhg() {
    local cur prev opts cmds
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="-n -t -C -q -v -h --json --fast --quiet --version --help"
    opts="$opts --type --context --max-candidates --color --no-ignore"
    opts="$opts --stats --min-score --exclude --hidden"
    cmds="info model"

    if [[ "${COMP_WORDS[1]}" == "model" ]]; then
        COMPREPLY=( $(compgen -W "install clean status --force" -- ${cur}) )
        return 0
    fi

    case "${prev}" in
        -t|--type)
            local types="py js ts rust rs go mojo java c cpp cs rb php sh md json yaml toml"
            COMPREPLY=( $(compgen -W "${types}" -- ${cur}) )
            return 0
            ;;
        --color)
            COMPREPLY=( $(compgen -W "auto always never" -- ${cur}) )
            return 0
            ;;
    esac

    if [[ ${cur} == -* ]] ; then
        COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
        return 0
    fi

    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${cmds}" -- ${cur}) $(compgen -d -- ${cur}) )
        return 0
    fi

    COMPREPLY=( $(compgen -d -- ${cur}) )
}
complete -F _hhg hhg
complete -F _hhg hygrep
"""

ZSH_COMPLETION = """
#compdef hhg hygrep

_hhg() {
    local -a commands
    commands=(
        'info:Show installation status'
        'model:Manage the reranking model'
    )

    if (( CURRENT == 2 )); then
        _describe -t commands 'command' commands
        _files -/
        return
    fi

    case "${words[2]}" in
        model)
            local -a model_cmds
            model_cmds=('install:Download' 'clean:Remove cache' 'status:Show status')
            _describe -t commands 'model command' model_cmds
            ;;
        *)
            _arguments -s \\
                '-n[Number of results]:count:' \\
                '-t[File type]:type:(py js ts rust rs go mojo java c cpp cs rb)' \\
                '--type[File type]:type:(py js ts rust rs go mojo java c cpp cs rb)' \\
                '-C[Context lines]:lines:' \\
                '--context[Context lines]:lines:' \\
                '-q[Quiet mode]' \\
                '--quiet[Quiet mode]' \\
                '-v[Version]' \\
                '--version[Version]' \\
                '-h[Help]' \\
                '--help[Help]' \\
                '--json[JSON output]' \\
                '--fast[Skip reranking]' \\
                '--max-candidates[Max to rerank]:count:' \\
                '--color[Color]:when:(auto always never)' \\
                '--no-ignore[Ignore .gitignore]' \\
                '--stats[Show stats]' \\
                '--min-score[Min score]:score:' \\
                '--exclude[Exclude pattern]:pattern:' \\
                '--hidden[Include hidden]' \\
                '*:directory:_files -/'
            ;;
    esac
}

_hhg "$@"
"""

FISH_COMPLETION = """
# Commands
complete -c hhg -n "__fish_use_subcommand" -a "info" -d "Show status"
complete -c hhg -n "__fish_use_subcommand" -a "model" -d "Manage model"

# Model subcommands
complete -c hhg -n "__fish_seen_subcommand_from model" -a "install" -d "Download"
complete -c hhg -n "__fish_seen_subcommand_from model" -a "clean" -d "Remove cache"
complete -c hhg -n "__fish_seen_subcommand_from model" -a "status" -d "Show status"

# Options
complete -c hhg -s n -d "Results count"
complete -c hhg -s t -l type -d "File type" -xa "py js ts rust rs go mojo java c cpp cs rb"
complete -c hhg -s C -l context -d "Context lines"
complete -c hhg -s q -l quiet -d "Quiet mode"
complete -c hhg -s v -l version -d "Version"
complete -c hhg -s h -l help -d "Help"
complete -c hhg -l json -d "JSON output"
complete -c hhg -l fast -d "Skip reranking"
complete -c hhg -l max-candidates -d "Max to rerank"
complete -c hhg -l color -d "Color" -xa "auto always never"
complete -c hhg -l no-ignore -d "Ignore .gitignore"
complete -c hhg -l stats -d "Show stats"
complete -c hhg -l min-score -d "Min score"
complete -c hhg -l exclude -d "Exclude pattern"
complete -c hhg -l hidden -d "Include hidden"
"""


def main():
    """Main entry point."""
    app()


if __name__ == "__main__":
    main()
