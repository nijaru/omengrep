"""hhg v2 - Semantic code search.

If you want grep, use rg. If you want semantic understanding, use hhg.
"""

import json
import time
from pathlib import Path

import typer
from rich.console import Console
from rich.panel import Panel

from . import __version__

# Consoles
console = Console()
err_console = Console(stderr=True)

# Exit codes
EXIT_MATCH = 0
EXIT_NO_MATCH = 1
EXIT_ERROR = 2

# Index directory
INDEX_DIR = ".hhg"

app = typer.Typer(
    name="hhg",
    help="Semantic code search",
    no_args_is_help=False,
    invoke_without_command=True,
    add_completion=False,
)


def get_index_path(root: Path) -> Path:
    """Get the index directory path."""
    return root.resolve() / INDEX_DIR


def find_index(search_path: Path) -> tuple[Path, Path | None]:
    """Find existing index by walking up directory tree.

    Returns:
        Tuple of (index_root, existing_index_dir or None).
    """
    from .semantic import find_index_root

    return find_index_root(search_path)


def index_exists(root: Path) -> bool:
    """Check if index exists for this directory."""
    index_path = get_index_path(root)
    return (index_path / "manifest.json").exists()


def build_index(root: Path, quiet: bool = False) -> None:
    """Build semantic index for directory."""
    from rich.progress import (
        BarColumn,
        Progress,
        SpinnerColumn,
        TaskProgressColumn,
        TextColumn,
    )

    from .scanner import scan
    from .semantic import SemanticIndex

    root = root.resolve()

    if quiet:
        # Quiet mode: no progress display
        files = scan(str(root), ".", include_hidden=False)
        if not files:
            return
        index = SemanticIndex(root)
        index.index(files)
        return

    # Interactive mode: show progress
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        console=err_console,
        transient=True,
    ) as progress:
        # Phase 1: Scan
        scan_task = progress.add_task("Scanning files...", total=None)
        t0 = time.perf_counter()
        files = scan(str(root), ".", include_hidden=False)
        scan_time = time.perf_counter() - t0
        progress.remove_task(scan_task)

        if not files:
            err_console.print("[yellow]No files found to index[/]")
            return

        err_console.print(f"[dim]Found {len(files)} files ({scan_time:.1f}s)[/]")

        # Phase 2: Extract and embed
        index = SemanticIndex(root)
        embed_task = progress.add_task("Embedding code blocks...", total=100)

        def on_progress(current: int, total: int, msg: str) -> None:
            if total > 0:
                progress.update(embed_task, completed=current, total=total)

        t0 = time.perf_counter()
        stats = index.index(files, on_progress=on_progress)
        index_time = time.perf_counter() - t0
        progress.remove_task(embed_task)

    # Summary
    err_console.print(
        f"[green]✓[/] Indexed {stats['blocks']} blocks "
        f"from {stats['files']} files ({index_time:.1f}s)"
    )
    if stats["skipped"]:
        err_console.print(f"[dim]  Skipped {stats['skipped']} unchanged files[/]")


def semantic_search(
    query: str,
    search_path: Path,
    index_root: Path,
    n: int = 10,
    threshold: float = 0.0,
) -> list[dict]:
    """Run semantic search.

    Args:
        query: Search query.
        search_path: Directory to search in (may be subdir of index_root).
        index_root: Root directory where index lives.
        n: Number of results.
        threshold: Minimum score filter.
    """
    from .semantic import SemanticIndex

    # Pass search_scope if searching a subdirectory
    index = SemanticIndex(index_root, search_scope=search_path)
    results = index.search(query, k=n)

    # Filter by threshold if specified (any non-zero value)
    if threshold != 0.0:
        results = [r for r in results if r.get("score", 0) >= threshold]

    return results


def grep_search(pattern: str, root: Path, regex: bool = False) -> list[dict]:
    """Fast grep search (escape hatch)."""
    from .extractor import ContextExtractor
    from .scanner import scan

    # Scan for matches
    files = scan(str(root), pattern, include_hidden=False)

    # Extract context from matches
    extractor = ContextExtractor()
    results = []

    for file_path, content in files.items():
        blocks = extractor.extract(file_path, pattern, content)
        for block in blocks:
            results.append(
                {
                    "file": file_path,
                    "type": block["type"],
                    "name": block["name"],
                    "line": block["start_line"],
                    "end_line": block["end_line"],
                    "content": block["content"],
                    "score": 1.0,  # No ranking for grep
                }
            )

    return results


def fast_search(
    query: str,
    root: Path,
    n: int = 10,
    max_candidates: int = 100,
) -> list[dict]:
    """Grep + neural rerank (no index required)."""
    from .reranker import Reranker
    from .scanner import scan

    # Scan for matches
    files = scan(str(root), query, include_hidden=False)

    if not files:
        return []

    # Rerank with neural model
    reranker = Reranker()
    results = reranker.search(query, files, top_k=n, max_candidates=max_candidates)

    # Normalize output format (start_line -> line)
    for r in results:
        if "start_line" in r:
            r["line"] = r.pop("start_line")

    return results


def filter_results(
    results: list[dict],
    file_types: str | None = None,
    exclude: list[str] | None = None,
) -> list[dict]:
    """Filter results by file type and exclude patterns."""
    import pathspec

    if not file_types and not exclude:
        return results

    # File type filtering
    if file_types:
        type_map = {
            "py": [".py", ".pyi"],
            "js": [".js", ".jsx", ".mjs"],
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
            "sh": [".sh", ".bash", ".zsh"],
            "md": [".md", ".markdown"],
            "json": [".json"],
            "yaml": [".yaml", ".yml"],
            "toml": [".toml"],
        }
        allowed_exts = set()
        for ft in file_types.split(","):
            ft = ft.strip().lower()
            if ft in type_map:
                allowed_exts.update(type_map[ft])
            else:
                allowed_exts.add(f".{ft}")
        results = [r for r in results if any(r["file"].endswith(ext) for ext in allowed_exts)]

    # Exclude pattern filtering
    if exclude:
        exclude_spec = pathspec.PathSpec.from_lines("gitwildmatch", exclude)
        results = [r for r in results if not exclude_spec.match_file(r["file"])]

    return results


def print_results(
    results: list[dict],
    json_output: bool = False,
    files_only: bool = False,
    compact: bool = False,
    show_content: bool = True,
    root: Path = None,
) -> None:
    """Print search results."""
    # Convert to relative paths
    if root:
        for r in results:
            try:
                r["file"] = str(Path(r["file"]).relative_to(root))
            except ValueError:
                pass

    # Files-only mode
    if files_only:
        seen = set()
        if json_output:
            files = []
            for r in results:
                if r["file"] not in seen:
                    files.append(r["file"])
                    seen.add(r["file"])
            print(json.dumps(files))
        else:
            for r in results:
                if r["file"] not in seen:
                    console.print(f"[cyan]{r['file']}[/]")
                    seen.add(r["file"])
        return

    if json_output:
        if compact:
            output = [{k: v for k, v in r.items() if k != "content"} for r in results]
        else:
            output = results
        print(json.dumps(output, indent=2))
        return

    for r in results:
        file_path = r["file"]
        type_str = f"[dim]{r.get('type', '')}[/]"
        name_str = r.get("name", "")
        line = r.get("line", 0)

        console.print(f"[cyan]{file_path}[/]:[yellow]{line}[/] {type_str} [bold]{name_str}[/]")

        # Content preview (first 3 non-empty lines)
        if show_content and r.get("content"):
            content_lines = [ln for ln in r["content"].split("\n") if ln.strip()][:3]
            for content_line in content_lines:
                # Truncate long lines
                if len(content_line) > 80:
                    content_line = content_line[:77] + "..."
                console.print(f"  [dim]{content_line}[/]")
            console.print()


@app.callback(invoke_without_command=True)
def main(
    ctx: typer.Context,
    query: str = typer.Argument(None, help="Search query"),
    path: Path = typer.Argument(Path("."), help="Directory to search"),
    # Output
    n: int = typer.Option(10, "-n", help="Number of results"),
    threshold: float = typer.Option(0.0, "--threshold", "--min-score", help="Minimum score (0-1)"),
    json_output: bool = typer.Option(False, "--json", "-j", help="JSON output"),
    files_only: bool = typer.Option(False, "-l", "--files-only", help="List files only"),
    compact: bool = typer.Option(False, "-c", "--compact", help="No content in output"),
    quiet: bool = typer.Option(False, "-q", "--quiet", help="Suppress progress"),
    # Filtering
    file_types: str = typer.Option(None, "-t", "--type", help="Filter types (py,js,ts)"),
    exclude: list[str] = typer.Option(None, "--exclude", help="Exclude glob pattern"),
    # Modes
    fast: bool = typer.Option(False, "-f", "--fast", help="Grep + rerank (no index)"),
    exact: bool = typer.Option(False, "-e", "--exact", help="Exact grep (no rerank)"),
    regex: bool = typer.Option(False, "-r", "--regex", help="Regex grep (no rerank)"),
    # Index control
    no_index: bool = typer.Option(False, "--no-index", help="Skip auto-index (fail if missing)"),
    # Meta
    version: bool = typer.Option(False, "-v", "--version", help="Show version"),
):
    """Semantic code search.

    Examples:
        hhg "authentication flow" ./src    # Semantic search (best quality)
        hhg -f "auth" ./src                # Grep + rerank (no index)
        hhg -e "TODO" ./src                # Exact grep (fastest)
        hhg -r "TODO.*fix" ./src           # Regex grep
    """
    if ctx.invoked_subcommand is not None:
        return

    # Handle case where user typed a subcommand name as query
    # (Typer can't distinguish due to optional positional args)
    if query == "status":
        err_console.print(f"[dim]Running: hhg status {path}[/]")
        status(path=path)
        raise typer.Exit()
    elif query == "rebuild":
        err_console.print(f"[dim]Running: hhg rebuild {path}[/]")
        rebuild(path=path, quiet=quiet)
        raise typer.Exit()
    elif query == "clean":
        err_console.print(f"[dim]Running: hhg clean {path}[/]")
        clean(path=path)
        raise typer.Exit()

    if version:
        console.print(f"hhg {__version__}")
        raise typer.Exit()

    if not query:
        console.print(
            Panel(
                "[bold]hhg[/] - Semantic code search\n\n"
                "[dim]Search modes:[/]\n"
                "  hhg <query> [path]       Semantic search (auto-indexes)\n"
                "  hhg -f <query> [path]    Grep + neural rerank (no index)\n"
                "  hhg -e <pattern> [path]  Exact grep (fastest)\n"
                "  hhg -r <pattern> [path]  Regex grep\n\n"
                "[dim]Index commands:[/]\n"
                "  hhg status [path]        Show index status\n"
                "  hhg rebuild [path]       Rebuild index\n"
                "  hhg clean [path]         Delete index",
                border_style="dim",
            )
        )
        raise typer.Exit()

    # Validate path
    path = path.resolve()
    if not path.exists():
        err_console.print(f"[red]Error:[/] Path does not exist: {path}")
        raise typer.Exit(EXIT_ERROR)

    # Escape hatches: grep mode
    if exact or regex:
        if not quiet:
            mode = "regex" if regex else "exact"
            err_console.print(f"[dim]Searching ({mode})...[/]")

        t0 = time.perf_counter()
        results = grep_search(query, path, regex=regex)
        search_time = time.perf_counter() - t0

        if not results:
            if not json_output:
                err_console.print("[dim]No matches found[/]")
            raise typer.Exit(EXIT_NO_MATCH)

        results = results[:n]
        results = filter_results(results, file_types, exclude)
        print_results(results, json_output, files_only, compact, root=path)

        if not quiet and not json_output and not files_only:
            err_console.print(f"[dim]{len(results)} results ({search_time:.2f}s)[/]")

        raise typer.Exit(EXIT_MATCH if results else EXIT_NO_MATCH)

    # Fast mode: grep + rerank (no index)
    if fast:
        if not quiet:
            err_console.print("[dim]Searching (grep + rerank)...[/]")

        t0 = time.perf_counter()
        results = fast_search(query, path, n=n)
        search_time = time.perf_counter() - t0

        if not results:
            if not json_output:
                err_console.print("[dim]No matches found[/]")
            raise typer.Exit(EXIT_NO_MATCH)

        # Filter by threshold
        if threshold != 0.0:
            results = [r for r in results if r.get("score", 0) >= threshold]

        results = filter_results(results, file_types, exclude)
        print_results(results, json_output, files_only, compact, root=path)

        if not quiet and not json_output and not files_only:
            err_console.print(f"[dim]{len(results)} results ({search_time:.2f}s)[/]")

        raise typer.Exit(EXIT_MATCH if results else EXIT_NO_MATCH)

    # Default: semantic search
    # Check omendb is available
    from .semantic import HAS_OMENDB

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed (required for semantic search)")
        err_console.print("Install with: pip install 'hygrep[semantic]'")
        err_console.print("\n[dim]Tip: Use -e for exact match without semantic search[/]")
        raise typer.Exit(EXIT_ERROR)

    # Walk up to find existing index, or determine where to create one
    index_root, existing_index = find_index(path)
    search_path = path  # May be a subdir of index_root

    # Check if index exists, build if not
    if existing_index is None:
        if no_index:
            err_console.print("[red]Error:[/] No index found (use without --no-index to build)")
            raise typer.Exit(EXIT_ERROR)
        if not quiet:
            err_console.print("[yellow]No index found. Building...[/]")
        # Build at search_path (becomes the new index_root)
        build_index(path, quiet=quiet)
        index_root = path
        if not quiet:
            err_console.print()
    elif not no_index:
        # Found existing index - check for stale files and auto-update
        from .scanner import scan
        from .semantic import SemanticIndex

        if not quiet and index_root != search_path:
            err_console.print(f"[dim]Using index at {index_root}[/]")

        files = scan(str(index_root), ".", include_hidden=False)
        index = SemanticIndex(index_root)
        stale_count = index.needs_update(files)

        if stale_count > 0:
            if not quiet:
                err_console.print(f"[dim]Updating index ({stale_count} files changed)...[/]")
            stats = index.update(files)
            if not quiet and stats.get("blocks", 0) > 0:
                err_console.print(f"[dim]  Updated {stats['blocks']} blocks[/]")

    # Run semantic search
    if not quiet:
        err_console.print(f"[dim]Searching for: {query}[/]")

    t0 = time.perf_counter()
    results = semantic_search(query, search_path, index_root, n=n, threshold=threshold)
    search_time = time.perf_counter() - t0

    if not results:
        if not json_output:
            err_console.print("[dim]No results found[/]")
        raise typer.Exit(EXIT_NO_MATCH)

    results = filter_results(results, file_types, exclude)
    print_results(results, json_output, files_only, compact, root=path)

    if not quiet and not json_output and not files_only:
        err_console.print(f"[dim]{len(results)} results ({search_time:.2f}s)[/]")

    raise typer.Exit(EXIT_MATCH if results else EXIT_NO_MATCH)


@app.command()
def status(path: Path = typer.Argument(Path("."), help="Directory")):
    """Show index status."""
    from .semantic import HAS_OMENDB, SemanticIndex

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed")
        err_console.print("Install with: pip install 'hygrep[semantic]'")
        raise typer.Exit(EXIT_ERROR)

    path = path.resolve()
    index_path = get_index_path(path)

    if not index_exists(path):
        err_console.print(f"[yellow]No index[/] at {index_path}")
        raise typer.Exit()

    index = SemanticIndex(path)
    count = index.count()
    console.print(f"[green]✓[/] Index: {count} vectors")
    console.print(f"  Location: {index_path}")


@app.command()
def rebuild(
    path: Path = typer.Argument(Path("."), help="Directory"),
    quiet: bool = typer.Option(False, "-q", "--quiet", help="Suppress progress"),
):
    """Rebuild index from scratch."""
    from .semantic import HAS_OMENDB, SemanticIndex

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed")
        raise typer.Exit(EXIT_ERROR)

    path = path.resolve()

    # Clear existing
    if index_exists(path):
        index = SemanticIndex(path)
        index.clear()
        if not quiet:
            err_console.print("[dim]Cleared existing index[/]")

    # Build fresh
    build_index(path, quiet=quiet)


@app.command()
def clean(path: Path = typer.Argument(Path("."), help="Directory")):
    """Delete index."""
    from .semantic import HAS_OMENDB, SemanticIndex

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed")
        raise typer.Exit(EXIT_ERROR)

    path = path.resolve()

    if not index_exists(path):
        err_console.print("[dim]No index to delete[/]")
        raise typer.Exit()

    index = SemanticIndex(path)
    index.clear()
    console.print("[green]✓[/] Index deleted")


def run():
    """Entry point."""
    app()


if __name__ == "__main__":
    run()
