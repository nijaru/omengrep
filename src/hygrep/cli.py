"""hhg - Semantic code search.

If you want grep, use rg. If you want semantic understanding, use hhg.
"""

import json
import os
import time
from pathlib import Path

import typer
from rich.console import Console
from rich.panel import Panel
from rich.status import Status

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

    # Interactive mode: show spinner for scanning
    with Status("Scanning files...", console=err_console):
        t0 = time.perf_counter()
        files = scan(str(root), ".", include_hidden=False)
        scan_time = time.perf_counter() - t0

    if not files:
        err_console.print("[yellow]No files found to index[/]")
        return

    err_console.print(f"[dim]Found {len(files)} files ({scan_time:.1f}s)[/]")

    # Phase 2: Extract and embed
    index = SemanticIndex(root)

    with Status("Indexing...", console=err_console):
        t0 = time.perf_counter()
        stats = index.index(files)
        index_time = time.perf_counter() - t0

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


def grep_search(pattern: str, root: Path) -> list[dict]:
    """Fast grep search (escape hatch).

    Note: Scanner uses POSIX regex, so both exact and regex modes
    work the same at this level. The distinction is for user clarity.
    """
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
def search(
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
    path_str = str(path)
    if query == "status":
        if path_str in ("--help", "-h"):
            console.print(
                "Usage: hhg status [PATH]\n\nShow index status for PATH (default: current dir)."
            )
            raise typer.Exit()
        err_console.print(f"[dim]Running: hhg status {path}[/]")
        status(path=path)
        raise typer.Exit()
    elif query == "build":
        if path_str in ("--help", "-h"):
            console.print(
                "Usage: hhg build [PATH] [--force] [-q]\n\n"
                "Build/update index for PATH (default: current dir)."
            )
            raise typer.Exit()
        # Handle --force flag
        force_build = path_str in ("--force", "-f")
        actual_path = Path(".") if force_build else path
        err_console.print(
            f"[dim]Running: hhg build {actual_path}{' --force' if force_build else ''}[/]"
        )
        build(path=actual_path, force=force_build, quiet=quiet)
        raise typer.Exit()
    elif query == "clean":
        if path_str in ("--help", "-h"):
            console.print(
                "Usage: hhg clean [PATH]\n\nDelete index for PATH (default: current dir)."
            )
            raise typer.Exit()
        err_console.print(f"[dim]Running: hhg clean {path}[/]")
        clean(path=path)
        raise typer.Exit()
    elif query == "model":
        # Handle model subcommand
        if path_str in ("--help", "-h"):
            console.print(
                "Usage: hhg model [COMMAND]\n\n"
                "Commands:\n"
                "  (none)     Show model status\n"
                "  install    Download/reinstall models"
            )
            raise typer.Exit()
        elif path_str == "install":
            err_console.print("[dim]Running: hhg model install[/]")
            install()
            raise typer.Exit()
        else:
            # Default: show model status
            err_console.print("[dim]Running: hhg model[/]")
            model_status(ctx)
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
                "  hhg status [path]     Show index status\n"
                "  hhg build [path]      Build/update index\n"
                "  hhg clean [path]      Delete index\n\n"
                "[dim]Model commands:[/]\n"
                "  hhg model             Show model status\n"
                "  hhg model install     Download/reinstall models",
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
        mode = "regex" if regex else "exact"
        if not quiet:
            with Status(f"Searching ({mode})...", console=err_console):
                t0 = time.perf_counter()
                results = grep_search(query, path)
                search_time = time.perf_counter() - t0
        else:
            t0 = time.perf_counter()
            results = grep_search(query, path)
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
            with Status("Searching (grep + rerank)...", console=err_console):
                t0 = time.perf_counter()
                results = fast_search(query, path, n=n)
                search_time = time.perf_counter() - t0
        else:
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
        err_console.print("Upgrade with: uv tool upgrade hygrep")
        err_console.print("\n[dim]Tip: Use -f for fast mode or -e for exact match[/]")
        raise typer.Exit(EXIT_ERROR)

    # Walk up to find existing index, or determine where to create one
    index_root, existing_index = find_index(path)
    search_path = path  # May be a subdir of index_root

    # Check if index exists
    if existing_index is None:
        # Check if auto-build is enabled via env var
        if os.environ.get("HHG_AUTO_BUILD", "").lower() in ("1", "true", "yes"):
            # Auto-build enabled
            if not quiet:
                err_console.print("[dim]Building index (HHG_AUTO_BUILD=1)...[/]")
            build_index(path, quiet=quiet)
            index_root = path
        else:
            # Require explicit build
            err_console.print("[red]Error:[/] No index found. Run 'hhg build' first.")
            err_console.print("[dim]Tip: Use -f for fast mode, or set HHG_AUTO_BUILD=1[/]")
            raise typer.Exit(EXIT_ERROR)

    if not no_index:
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
                with Status(f"Updating {stale_count} changed files...", console=err_console):
                    stats = index.update(files)
                if stats.get("blocks", 0) > 0:
                    err_console.print(f"[dim]  Updated {stats['blocks']} blocks[/]")
            else:
                index.update(files)

    # Run semantic search
    if not quiet:
        with Status(f"Searching for: {query}...", console=err_console):
            t0 = time.perf_counter()
            results = semantic_search(query, search_path, index_root, n=n, threshold=threshold)
            search_time = time.perf_counter() - t0
    else:
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
    from .scanner import scan
    from .semantic import HAS_OMENDB, SemanticIndex

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed")
        err_console.print("Upgrade with: uv tool upgrade hygrep")
        raise typer.Exit(EXIT_ERROR)

    path = path.resolve()

    if not index_exists(path):
        console.print("No index found. Run 'hhg build' to create.")
        raise typer.Exit()

    index = SemanticIndex(path)
    block_count = index.count()

    # Get file count from manifest
    manifest = index._load_manifest()
    file_count = len(manifest.get("files", {}))

    # Check for stale files
    files = scan(str(path), ".", include_hidden=False)
    changed, deleted = index.get_stale_files(files)
    stale_count = len(changed) + len(deleted)

    if stale_count == 0:
        console.print(f"[green]✓[/] {file_count} files, {block_count} blocks (up to date)")
    else:
        parts = []
        if changed:
            parts.append(f"{len(changed)} changed")
        if deleted:
            parts.append(f"{len(deleted)} deleted")
        stale_str = ", ".join(parts)
        console.print(f"[yellow]![/] {file_count} files, {block_count} blocks ({stale_str})")
        console.print("  Run 'hhg build' to update")


@app.command()
def build(
    path: Path = typer.Argument(Path("."), help="Directory"),
    force: bool = typer.Option(
        False, "--force", "-f", help="Force rebuild or override parent index"
    ),
    quiet: bool = typer.Option(False, "-q", "--quiet", help="Suppress progress"),
):
    """Build or update index.

    By default, does an incremental update (only changed files).
    Use --force to rebuild from scratch or create separate index when parent exists.
    """
    import shutil

    from .scanner import scan
    from .semantic import HAS_OMENDB, SemanticIndex, find_parent_index, find_subdir_indexes

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed")
        err_console.print("Upgrade with: uv tool upgrade hygrep")
        raise typer.Exit(EXIT_ERROR)

    path = path.resolve()

    # Check for parent index that already covers this path
    if not index_exists(path):
        parent = find_parent_index(path)
        if parent and not force:
            if not quiet:
                err_console.print(f"[dim]Using parent index at {parent}[/]")
            path = parent

    # Find subdir indexes that will be superseded
    subdir_indexes = find_subdir_indexes(path)

    if force and index_exists(path):
        # Full rebuild: clear first
        index = SemanticIndex(path)
        index.clear()
        if not quiet:
            err_console.print("[dim]Cleared existing index[/]")
        build_index(path, quiet=quiet)
    elif index_exists(path):
        # Incremental update
        if not quiet:
            with Status("Scanning files...", console=err_console):
                files = scan(str(path), ".", include_hidden=False)
        else:
            files = scan(str(path), ".", include_hidden=False)

        index = SemanticIndex(path)
        changed, deleted = index.get_stale_files(files)
        stale_count = len(changed) + len(deleted)

        if stale_count == 0:
            if not quiet:
                console.print("[green]✓[/] Index up to date")
            # Fall through to clean up subdir indexes if any
        else:
            if not quiet:
                with Status(f"Updating {stale_count} files...", console=err_console):
                    stats = index.update(files)
            else:
                stats = index.update(files)

            if not quiet:
                console.print(
                    f"[green]✓[/] Updated {stats.get('blocks', 0)} blocks "
                    f"from {stats.get('files', 0)} files"
                )
                if stats.get("deleted", 0):
                    console.print(f"  [dim]Removed {stats['deleted']} stale blocks[/]")
    else:
        # No index exists, build fresh
        # First, merge any subdir indexes (much faster than re-embedding)
        merged_any = False
        if subdir_indexes:
            if not quiet:
                err_console.print(f"[dim]Merging {len(subdir_indexes)} subdir index(es)...[/]")
            index = SemanticIndex(path)
            total_merged = 0
            for idx in subdir_indexes:
                merge_stats = index.merge_from_subdir(idx)
                total_merged += merge_stats.get("merged", 0)
            if total_merged > 0:
                merged_any = True
                if not quiet:
                    err_console.print(f"[dim]  Merged {total_merged} blocks from subdir indexes[/]")

        # Build (will skip files already merged via hash matching)
        build_index(path, quiet=quiet)

        # If we merged, clean up any deleted files from merged manifests
        if merged_any:
            # Reopen index to get fresh state after build
            index = SemanticIndex(path)
            files = scan(str(path), ".", include_hidden=False)
            _changed, deleted = index.get_stale_files(files)
            if deleted:
                index.update(files)
                if not quiet:
                    err_console.print(f"[dim]  Cleaned up {len(deleted)} deleted file entries[/]")

    # Clean up subdir indexes (now superseded by parent)
    for idx in subdir_indexes:
        shutil.rmtree(idx)
        if not quiet:
            err_console.print(f"[dim]Removed superseded index: {idx.parent.relative_to(path)}[/]")


@app.command()
def clean(path: Path = typer.Argument(Path("."), help="Directory")):
    """Delete index."""
    from .semantic import HAS_OMENDB, SemanticIndex

    if not HAS_OMENDB:
        err_console.print("[red]Error:[/] omendb not installed")
        err_console.print("Upgrade with: uv tool upgrade hygrep")
        raise typer.Exit(EXIT_ERROR)

    path = path.resolve()

    if not index_exists(path):
        err_console.print("[dim]No index to delete[/]")
        raise typer.Exit()

    index = SemanticIndex(path)
    index.clear()
    console.print("[green]✓[/] Index deleted")


# Model command group
model_app = typer.Typer(
    name="model",
    help="Manage models",
    no_args_is_help=False,
    invoke_without_command=True,
)
app.add_typer(model_app, name="model")


def _get_model_status() -> list[dict]:
    """Get status of all models."""
    from huggingface_hub import try_to_load_from_cache

    from .embedder import MODEL_FILE as EMBED_FILE
    from .embedder import MODEL_REPO as EMBED_REPO
    from .embedder import TOKENIZER_FILE as EMBED_TOKENIZER
    from .reranker import MODEL_FILE as RERANK_FILE
    from .reranker import MODEL_REPO as RERANK_REPO
    from .reranker import TOKENIZER_FILE as RERANK_TOKENIZER

    models = []

    # Check embedder
    embed_model = try_to_load_from_cache(EMBED_REPO, EMBED_FILE)
    embed_tokenizer = try_to_load_from_cache(EMBED_REPO, EMBED_TOKENIZER)
    embed_installed = embed_model is not None and embed_tokenizer is not None
    models.append(
        {
            "name": "embedder",
            "repo": EMBED_REPO,
            "installed": embed_installed,
        }
    )

    # Check reranker
    rerank_model = try_to_load_from_cache(RERANK_REPO, RERANK_FILE)
    rerank_tokenizer = try_to_load_from_cache(RERANK_REPO, RERANK_TOKENIZER)
    rerank_installed = rerank_model is not None and rerank_tokenizer is not None
    models.append(
        {
            "name": "reranker",
            "repo": RERANK_REPO,
            "installed": rerank_installed,
        }
    )

    return models


@model_app.callback(invoke_without_command=True)
def model_status(ctx: typer.Context) -> None:
    """Show model status."""
    if ctx.invoked_subcommand is not None:
        return

    models = _get_model_status()

    all_installed = all(m["installed"] for m in models)

    if all_installed:
        console.print("[green]✓[/] All models installed")
    else:
        console.print("[yellow]![/] Some models missing")

    for m in models:
        status = "[green]✓[/]" if m["installed"] else "[red]✗[/]"
        console.print(f"  {status} {m['name']}: {m['repo']}")

    if not all_installed:
        console.print("\nRun 'hhg model install' to download missing models")


@model_app.command()
def install() -> None:
    """Download or reinstall models."""
    from huggingface_hub import hf_hub_download

    from .embedder import MODEL_FILE as EMBED_FILE
    from .embedder import MODEL_REPO as EMBED_REPO
    from .embedder import TOKENIZER_FILE as EMBED_TOKENIZER
    from .reranker import MODEL_FILE as RERANK_FILE
    from .reranker import MODEL_REPO as RERANK_REPO
    from .reranker import TOKENIZER_FILE as RERANK_TOKENIZER

    models = [
        ("embedder", EMBED_REPO, [EMBED_FILE, EMBED_TOKENIZER]),
        ("reranker", RERANK_REPO, [RERANK_FILE, RERANK_TOKENIZER]),
    ]

    for name, repo, files in models:
        console.print(f"[dim]Downloading {name} ({repo})...[/]")
        for filename in files:
            hf_hub_download(repo_id=repo, filename=filename, force_download=True)

    console.print("[green]✓[/] All models installed")


def main():
    """Entry point."""
    app()


if __name__ == "__main__":
    main()
