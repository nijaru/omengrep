"""hhg - Semantic code search.

Hybrid search combining BM25 (keywords) and semantic similarity (embeddings).
For grep, use ripgrep. For semantic understanding, use hhg.
"""

import json
import os
import time
from contextlib import nullcontext
from pathlib import Path

import typer
from rich.console import Console
from rich.status import Status

from . import __version__
from .semantic import INDEX_DIR, find_index_root

# Consoles
console = Console()
err_console = Console(stderr=True)

# Exit codes
EXIT_MATCH = 0
EXIT_NO_MATCH = 1
EXIT_ERROR = 2


app = typer.Typer(
    name="hhg",
    help="Semantic code search",
    no_args_is_help=False,
    invoke_without_command=True,
    context_settings={
        "allow_extra_args": True,
        "ignore_unknown_options": True,
        "allow_interspersed_args": True,
    },
)


def get_index_path(root: Path) -> Path:
    """Get the index directory path."""
    return root.resolve() / INDEX_DIR


def index_exists(root: Path) -> bool:
    """Check if index exists for this directory."""
    index_path = get_index_path(root)
    return (index_path / "manifest.json").exists()


def _handle_index_needs_rebuild(index_root: Path, quiet: bool = False) -> bool:
    """Handle IndexNeedsRebuild by prompting user to rebuild.

    Returns True if rebuild was performed, False if user declined.
    """
    if quiet:
        err_console.print("[red]✗[/] Index needs rebuild. Run: hhg build --force")
        return False

    err_console.print("[yellow]![/] Index needs rebuild.")
    try:
        response = input("Rebuild now? [Y/n] ").strip().lower()
    except (KeyboardInterrupt, EOFError):
        err_console.print()
        return False

    if response in ("", "y", "yes"):
        build_index(index_root, quiet=quiet, force=True)
        return True
    return False


def parse_file_reference(query: str) -> tuple[str, int | None, str | None] | None:
    """Parse file reference from query if it looks like file#name or file:line.

    Returns:
        (file_path, line, name) if query is a valid file reference, None otherwise.
        - file#name -> (file, None, name)
        - file:42 -> (file, 42, None)
        - file (exists) -> (file, None, None)
    """
    if not query:
        return None

    # Check for #name syntax first (less common in text)
    if "#" in query:
        file_part, name = query.rsplit("#", 1)
        # Validate name looks like an identifier
        if name and name.replace(".", "").replace("_", "").isalnum():
            if Path(file_part).exists():
                return (file_part, None, name)

    # Check for :line syntax
    if ":" in query:
        parts = query.rsplit(":", 1)
        if parts[1].isdigit():
            if Path(parts[0]).exists():
                return (parts[0], int(parts[1]), None)

    # Check for plain file path (must exist to avoid treating text as file ref)
    if Path(query).exists() and Path(query).is_file():
        return (query, None, None)

    return None


def build_index(
    root: Path,
    quiet: bool = False,
    workers: int = 0,
    batch_size: int = 128,
    merge_info: list[tuple[str, int]] | None = None,
    defer_summary: bool = False,
) -> tuple[dict, float] | None:
    """Build semantic index for directory.

    Returns:
        If defer_summary=True, returns (stats, index_time) for caller to print.
        Otherwise returns None.
    """
    from .scanner import scan
    from .semantic import SemanticIndex

    root = root.resolve()

    try:
        if quiet:
            # Quiet mode: no progress display
            files = scan(str(root), ".", include_hidden=False)
            if not files:
                return
            index = SemanticIndex(root)
            stats = index.index(files, workers=workers, batch_size=batch_size)
            if stats.get("errors", 0) > 0:
                err_console.print(f"[yellow]![/] {stats['errors']} files failed to index")
            return

        # Interactive mode: show spinner for scanning
        with Status("Scanning files...", console=err_console):
            files = scan(str(root), ".", include_hidden=False)

        if not files:
            err_console.print("[yellow]![/] No files found to index")
            return

        err_console.print(f"[dim]Found {len(files)} files[/]")

        # Print merge info if any subdirs were merged
        if merge_info:
            for subdir, blocks in merge_info:
                err_console.print(f"[dim]    Merged {blocks} blocks from {subdir}[/]")

        # Phase 2: Extract and embed
        index = SemanticIndex(root)
        t0 = time.perf_counter()

        # Use progress bar for large builds (50+ files), spinner for small
        if len(files) >= 50:
            from rich.progress import (
                BarColumn,
                Progress,
                TaskProgressColumn,
                TextColumn,
                TimeElapsedColumn,
            )

            with Progress(
                TextColumn("[progress.description]{task.description}"),
                BarColumn(),
                TaskProgressColumn(),
                TimeElapsedColumn(),
                console=err_console,
                transient=True,
            ) as progress:
                task = progress.add_task("Extracting...", total=None)

                def on_progress(current: int, total: int, _msg: str) -> None:
                    if progress.tasks[task].total != total:
                        progress.update(task, total=total, description="Embedding...")
                    progress.update(task, completed=current)

                stats = index.index(
                    files, workers=workers, batch_size=batch_size, on_progress=on_progress
                )
        else:
            with Status("Indexing...", console=err_console):
                stats = index.index(files, workers=workers, batch_size=batch_size)

        index_time = time.perf_counter() - t0

        # Summary: skipped first, then final count
        if stats["skipped"]:
            err_console.print(f"[dim]    Skipped {stats['skipped']} unchanged files[/]")

        # Return stats for caller to print summary, or print it here
        if defer_summary:
            return stats, index_time

        err_console.print(
            f"  [green]✓[/] Indexed {stats['blocks']} blocks "
            f"from {stats['files']} files ({index_time:.1f}s)"
        )
        if stats.get("errors", 0) > 0:
            err_console.print(f"[yellow]![/] {stats['errors']} files failed to index")
        return None

    except KeyboardInterrupt:
        # Partial index is preserved - next build will resume
        err_console.print("\n[yellow]![/] Interrupted. Progress saved, run 'hhg build' to resume")
        raise typer.Exit(130)
    except RuntimeError as e:
        # Model loading errors from embedder
        err_console.print(f"[red]✗[/] {e}")
        raise typer.Exit(EXIT_ERROR)
    except PermissionError as e:
        err_console.print(f"[red]✗[/] Permission denied: {e.filename}")
        err_console.print("[dim]Check directory permissions[/]")
        raise typer.Exit(EXIT_ERROR)
    except OSError as e:
        if "No space left" in str(e) or e.errno == 28:
            err_console.print("[red]✗[/] No disk space left")
        else:
            err_console.print(f"[red]✗[/] {e}")
        raise typer.Exit(EXIT_ERROR)


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


def filter_results(
    results: list[dict],
    file_types: str | None = None,
    exclude: list[str] | None = None,
    code_only: bool = False,
) -> list[dict]:
    """Filter results by file type and exclude patterns."""
    import pathspec

    # Add doc extensions to exclude if code_only
    if code_only:
        doc_patterns = ["*.md", "*.markdown", "*.txt", "*.rst", "*.adoc"]
        exclude = list(exclude or []) + doc_patterns

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


def boost_results(results: list[dict], query: str) -> list[dict]:
    """Apply code-aware ranking boosts to search results.

    Boosts:
        - Exact name match: 2.5x
        - Term overlap: +30% per matching term (camelCase/snake_case aware)
        - Type match: 1.5x if query mentions the type (e.g., "class", "function")
        - Type hierarchy: class 1.3x, function 1.2x (fallback if no type in query)
        - File path relevance: 1.15x
        - Max total boost capped at 4x to prevent over-boosting
    """
    import re

    if not results or not query:
        return results

    query_lower = query.lower()

    # Split camelCase and snake_case, then tokenize
    # "getUserData" → "get user data", "get_user_data" → "get user data"
    expanded = re.sub(r"([a-z])([A-Z])", r"\1 \2", query_lower)
    query_terms = set(re.split(r"[\s_\-./]+", expanded))
    query_terms.discard("")

    # Common short terms meaningful in code
    short_whitelist = {"db", "fs", "io", "ui", "id", "ok", "fn", "rx", "tx", "api"}
    query_terms = {t for t in query_terms if len(t) >= 3 or t in short_whitelist}

    # Detect if user mentioned a specific type in query
    query_wants_class = any(t in query_terms for t in ("class", "struct", "type"))
    query_wants_func = any(t in query_terms for t in ("function", "func", "fn", "method", "def"))

    type_weights = {
        "class": 1.3,
        "struct": 1.3,
        "function": 1.2,
        "method": 1.2,
        "interface": 1.1,
        "type": 1.1,
        "trait": 1.1,
        "enum": 1.1,
    }

    for r in results:
        boost = 1.0
        name = r.get("name", "").lower()
        block_type = r.get("type", "").lower()
        file_path = r.get("file", "").lower()

        # Expand name to handle camelCase/snake_case
        name_expanded = re.sub(r"([a-z])([A-Z])", r"\1 \2", name)
        name_terms = set(re.split(r"[\s_\-./]+", name_expanded))
        name_terms.discard("")

        # 1. Name matching
        if name and name in query_terms:
            # Exact full name match
            boost *= 2.5
        else:
            # Term overlap (how many query terms appear in name)
            overlap = query_terms & name_terms
            if overlap:
                boost *= 1.0 + (0.3 * len(overlap))  # +30% per matching term

        # 2. Type boost - context-aware
        if query_wants_class and block_type in ("class", "struct"):
            boost *= 1.5
        elif query_wants_func and block_type in ("function", "method"):
            boost *= 1.5
        elif not query_wants_class and not query_wants_func:
            # Fallback: apply default type hierarchy
            boost *= type_weights.get(block_type, 1.0)

        # 3. File path relevance
        if any(term in file_path for term in query_terms if len(term) >= 3):
            boost *= 1.15

        # Cap total boost to prevent over-boosting mediocre semantic matches
        boost = min(boost, 4.0)

        r["score"] = r.get("score", 0) * boost

    results.sort(key=lambda x: x.get("score", 0), reverse=True)
    return results


def print_results(
    results: list[dict],
    json_output: bool = False,
    files_only: bool = False,
    compact: bool = False,
    show_content: bool = True,
    show_score: bool = False,
    root: Path | None = None,
) -> None:
    """Print search results.

    Args:
        show_score: If True, display similarity percentage (for similar search).
    """
    # Convert to relative paths
    if root:
        for r in results:
            try:
                r["file"] = str(Path(r["file"]).relative_to(root))
            except ValueError:
                pass

    # Files-only mode
    if files_only:
        unique_files = list(dict.fromkeys(r["file"] for r in results))
        if json_output:
            print(json.dumps(unique_files))
        else:
            for f in unique_files:
                console.print(f"[cyan]{f}[/]")
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

        # Build the output line
        line_parts = [f"[cyan]{file_path}[/]:[yellow]{line}[/] {type_str} [bold]{name_str}[/]"]

        # Add similarity score if requested
        if show_score and "score" in r:
            score_pct = int(r["score"] * 100)
            line_parts.append(f"[magenta]({score_pct}% similar)[/]")

        console.print(" ".join(line_parts))

        # Content preview (first 3 non-empty lines)
        if show_content and r.get("content"):
            content_lines = [ln for ln in r["content"].split("\n") if ln.strip()][:3]
            for content_line in content_lines:
                # Truncate long lines
                if len(content_line) > 80:
                    content_line = content_line[:77] + "..."
                console.print(f"  [dim]{content_line}[/]")
            console.print()


def _run_similar_search(
    file_path: str,
    line: int | None = None,
    name: str | None = None,
    n: int = 10,
    json_output: bool = False,
    files_only: bool = False,
    compact: bool = False,
    quiet: bool = False,
) -> None:
    """Run similar search for a file reference."""
    from .semantic import (
        AmbiguousBlockError,
        BlockNotFoundError,
        IndexNeedsRebuild,
        SemanticIndex,
    )

    file_path = str(Path(file_path).resolve())

    # Find index by walking up from the file's directory
    file_dir = Path(file_path).parent
    index_root, existing_index = find_index_root(file_dir)

    if existing_index is None:
        err_console.print("[red]✗[/] No index found. Run 'hhg build' first.")
        raise typer.Exit(EXIT_ERROR)

    # Build reference description for output
    if name:
        ref_desc = f"{Path(file_path).name}#{name}"
    elif line:
        ref_desc = f"{Path(file_path).name}:{line}"
    else:
        ref_desc = Path(file_path).name

    try:
        ctx = (
            nullcontext()
            if quiet
            else Status(f"Finding similar to {ref_desc}...", console=err_console)
        )
        with ctx:
            index = SemanticIndex(index_root)
            results = index.find_similar(file_path, line=line, name=name, k=n)
    except IndexNeedsRebuild:
        if _handle_index_needs_rebuild(index_root, quiet=quiet):
            index = SemanticIndex(index_root)
            results = index.find_similar(file_path, line=line, name=name, k=n)
        else:
            raise typer.Exit(EXIT_ERROR)
    except BlockNotFoundError as e:
        err_console.print(f"[red]✗[/] {e}")
        raise typer.Exit(EXIT_ERROR)
    except AmbiguousBlockError as e:
        err_console.print(f"[red]✗[/] Multiple blocks named '{e.name}' found:")
        for m in e.matches:
            err_console.print(f"  - line {m['line']}: {m['type']} {m['name']}")
        err_console.print(f"\nUse {Path(file_path).name}:<line> to specify.")
        raise typer.Exit(EXIT_ERROR)
    except RuntimeError as e:
        err_console.print(f"[red]✗[/] {e}")
        raise typer.Exit(EXIT_ERROR)

    if not results:
        if not json_output:
            err_console.print("[dim]No similar code found[/]")
        raise typer.Exit(EXIT_NO_MATCH)

    print_results(
        results,
        json_output=json_output,
        files_only=files_only,
        compact=compact,
        show_score=True,
        root=index_root,
    )

    if not quiet and not json_output:
        result_word = "result" if len(results) == 1 else "results"
        err_console.print(f"[dim]{len(results)} similar {result_word}[/]")


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
    code_only: bool = typer.Option(False, "--code-only", help="Exclude docs (md, txt, rst)"),
    # Index control
    no_index: bool = typer.Option(False, "--no-index", help="Skip auto-index (fail if missing)"),
    # Meta
    version: bool = typer.Option(False, "-v", "--version", help="Show version"),
    # Hidden options for subcommand passthrough
    recursive: bool = typer.Option(False, "--recursive", "-r", hidden=True),
    workers: int = typer.Option(0, hidden=True),
    batch_size: int = typer.Option(128, hidden=True),
):
    """Semantic code search.

    Examples:
        hhg "authentication flow" ./src    # Semantic search
        hhg "error handling" -t py         # Filter by file type
        hhg build ./src                    # Build index first
    """
    if ctx.invoked_subcommand is not None:
        return

    # Handle case where user typed a subcommand name as query
    # (Typer can't distinguish due to optional positional args)
    if query == "status":
        if _check_help_flag():
            console.print(
                "Usage: hhg status [PATH]\n\nShow index status for PATH (default: current dir)."
            )
            raise typer.Exit()
        actual_path, _ = _parse_subcommand_args(path)
        err_console.print(f"[dim]Running: hhg status {actual_path}[/]")
        status(path=actual_path)
        raise typer.Exit()

    elif query == "build":
        if _check_help_flag():
            console.print(
                "Usage: hhg build [PATH] [--force] [-q]\n\n"
                "Build/update index for PATH (default: current dir)."
            )
            raise typer.Exit()
        actual_path, flags = _parse_subcommand_args(path, {"force": False, "quiet": quiet})
        if not flags["quiet"]:
            err_console.print(
                f"[dim]Running: hhg build {actual_path}{' --force' if flags['force'] else ''}[/]"
            )
        build(path=actual_path, force=flags["force"], quiet=flags["quiet"])
        raise typer.Exit()

    elif query == "clean":
        if _check_help_flag():
            console.print(
                "Usage: hhg clean [PATH] [-r/--recursive]\n\n"
                "Delete index for PATH (default: current dir).\n"
                "Use -r/--recursive to also delete indexes in subdirectories.\n\n"
                "Examples:\n"
                "  hhg clean              # Delete index in current dir\n"
                "  hhg clean ./src        # Delete index in ./src\n"
                "  hhg clean -r           # Delete all indexes recursively\n"
                "  hhg clean ./src -r     # Delete indexes in ./src recursively"
            )
            raise typer.Exit()
        actual_path, flags = _parse_subcommand_args(path, {"recursive": recursive})
        flag_str = " --recursive" if flags["recursive"] else ""
        err_console.print(f"[dim]Running: hhg clean {actual_path}{flag_str}[/]")
        clean(path=actual_path, recursive=flags["recursive"])
        raise typer.Exit()

    elif query == "list":
        if _check_help_flag():
            console.print(
                "Usage: hhg list [PATH]\n\nList all indexes under PATH (default: current dir)."
            )
            raise typer.Exit()
        actual_path, _ = _parse_subcommand_args(path)
        err_console.print(f"[dim]Running: hhg list {actual_path}[/]")
        list_indexes(path=actual_path)
        raise typer.Exit()

    elif query == "model":
        # Check if next arg is "install"
        args = _subcommand_original_argv[1:] if _subcommand_original_argv else []
        if args and args[0] == "install":
            if _check_help_flag():
                console.print(
                    "Usage: hhg model install\n\n"
                    "Download embedding model for offline use or to fix corrupted download."
                )
                raise typer.Exit()
            err_console.print("[dim]Running: hhg model install[/]")
            model_install()
            raise typer.Exit()
        else:
            if _check_help_flag():
                console.print(
                    "Usage: hhg model [install]\n\n"
                    "Show model status, or install with 'hhg model install'."
                )
                raise typer.Exit()
            err_console.print("[dim]Running: hhg model[/]")
            model()
            raise typer.Exit()

    elif query == "similar":
        # Deprecated: use hhg file#name or hhg file:line instead
        args = _subcommand_original_argv[1:] if _subcommand_original_argv else []
        if _check_help_flag():
            console.print(
                "Usage: hhg <file>#<name> or hhg <file>:<line>\n\n"
                "Find code similar to a given file or block.\n\n"
                "Note: 'hhg similar' is deprecated. Use directly:\n"
                "  hhg src/auth.py#login          # Similar to function by name\n"
                "  hhg src/auth.py:42             # Similar to block at line 42\n"
                "  hhg src/auth.py                # Similar to first block\n\n"
                "Options:\n"
                "  -n N        Number of results (default: 10)\n"
                "  -j, --json  JSON output\n"
                "  -c          Compact output\n"
                "  -q          Quiet mode"
            )
            raise typer.Exit()
        # Parse args and options
        n_val = 10
        json_val = json_output
        compact_val = compact
        quiet_val = quiet
        positionals = []
        i = 0
        while i < len(args):
            arg = args[i]
            if arg == "-n" and i + 1 < len(args):
                n_val = int(args[i + 1])
                i += 2
            elif arg in ("-j", "--json"):
                json_val = True
                i += 1
            elif arg in ("-c", "--compact"):
                compact_val = True
                i += 1
            elif arg in ("-q", "--quiet"):
                quiet_val = True
                i += 1
            elif not arg.startswith("-"):
                positionals.append(arg)
                i += 1
            else:
                i += 1
        file_arg = positionals[0] if positionals else None
        if not file_arg:
            err_console.print("[red]✗[/] Missing file path")
            raise typer.Exit(EXIT_ERROR)
        # Parse file reference
        file_ref = parse_file_reference(file_arg)
        if file_ref is None:
            # Treat as file path if it doesn't exist yet
            file_ref = (file_arg, None, None)
        file_path, line, name = file_ref
        if not quiet_val:
            err_console.print(
                "[dim]Note: 'hhg similar' is deprecated. Use 'hhg file#name' directly.[/]"
            )
        _run_similar_search(
            file_path=file_path,
            line=line,
            name=name,
            n=n_val,
            json_output=json_val,
            compact=compact_val,
            quiet=quiet_val,
        )
        raise typer.Exit()

    if version:
        console.print(f"hhg {__version__}")
        raise typer.Exit()

    # Handle -h or no query as help (typer parses -h as query since it's first positional)
    if not query or query in ("-h", "--help"):
        console.print(ctx.get_help())
        raise typer.Exit()

    # Check if query is a file reference (file#name, file:line, or existing file)
    file_ref = parse_file_reference(query)
    if file_ref is not None:
        # This is a similar search, not a text search
        file_path, line, name = file_ref
        _run_similar_search(
            file_path=file_path,
            line=line,
            name=name,
            n=n,
            json_output=json_output,
            files_only=files_only,
            compact=compact,
            quiet=quiet,
        )
        raise typer.Exit()

    # Validate path
    path = path.resolve()
    if not path.exists():
        err_console.print(f"[red]✗[/] Path does not exist: {path}")
        raise typer.Exit(EXIT_ERROR)

    # Walk up to find existing index, or determine where to create one
    index_root, existing_index = find_index_root(path)
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
            err_console.print("[red]✗[/] No index found. Run 'hhg build' first.")
            err_console.print("[dim]Tip: Set HHG_AUTO_BUILD=1 for auto-indexing[/]")
            raise typer.Exit(EXIT_ERROR)

    if not no_index:
        # Found existing index - check for stale files and auto-update
        from .scanner import scan
        from .semantic import IndexNeedsRebuild, SemanticIndex

        if not quiet and index_root != search_path:
            err_console.print(f"[dim]Using index at {index_root}[/]")

        files = scan(str(index_root), ".", include_hidden=False)
        try:
            index = SemanticIndex(index_root)
        except IndexNeedsRebuild:
            if _handle_index_needs_rebuild(index_root, quiet=quiet):
                index = SemanticIndex(index_root)
            else:
                raise typer.Exit(EXIT_ERROR)
        try:
            stale_count = index.needs_update(files)

            if stale_count > 0:
                if not quiet:
                    with Status(f"Updating {stale_count} changed files...", console=err_console):
                        stats = index.update(files)
                    if stats.get("blocks", 0) > 0:
                        err_console.print(f"[dim]  Updated {stats['blocks']} blocks[/]")
                else:
                    index.update(files)
        except RuntimeError as e:
            err_console.print(f"[red]✗[/] {e}")
            # Provide context-aware help if using a parent index
            if index_root != search_path:
                err_console.print("\nOptions:")
                err_console.print(f"  hhg build --force {index_root}  # Rebuild parent index")
                err_console.print(
                    f"  hhg build --force {search_path}  # Create separate index here"
                )
            raise typer.Exit(EXIT_ERROR)

        # Release lock before search
        index.close()

    # Run semantic search
    ctx = nullcontext() if quiet else Status(f"Searching for: {query}...", console=err_console)
    with ctx:
        t0 = time.perf_counter()
        results = semantic_search(query, search_path, index_root, n=n, threshold=threshold)
        search_time = time.perf_counter() - t0

    if not results:
        if not json_output:
            err_console.print("[dim]No results found[/]")
        raise typer.Exit(EXIT_NO_MATCH)

    results = filter_results(results, file_types, exclude, code_only)
    results = boost_results(results, query)
    print_results(results, json_output, files_only, compact, root=path)

    if not quiet and not json_output and not files_only:
        result_word = "result" if len(results) == 1 else "results"
        err_console.print(f"[dim]{len(results)} {result_word} ({search_time:.2f}s)[/]")

    raise typer.Exit(EXIT_MATCH if results else EXIT_NO_MATCH)


@app.command()
def status(path: Path = typer.Argument(Path("."), help="Directory")):
    """Show index status."""
    from .scanner import scan
    from .semantic import IndexNeedsRebuild, SemanticIndex

    path = path.resolve()

    if not index_exists(path):
        console.print("[yellow]![/] No index. Run 'hhg build' to create.")
        raise typer.Exit()

    index = SemanticIndex(path)
    try:
        block_count = index.count()
        # Get file count from manifest
        manifest = index._load_manifest()
        file_count = len(manifest.get("files", {}))
        # Check for stale files
        files = scan(str(path), ".", include_hidden=False)
        changed, deleted = index.get_stale_files(files)
    except IndexNeedsRebuild:
        console.print("[yellow]![/] Index needs rebuild. Run: hhg build --force")
        raise typer.Exit()
    except RuntimeError as e:
        err_console.print(f"[red]✗[/] {e}")
        raise typer.Exit(EXIT_ERROR)
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
    from .semantic import (
        IndexNeedsRebuild,
        SemanticIndex,
        find_parent_index,
        find_subdir_indexes,
    )

    path = path.resolve()
    original_path = path  # Track original request for error messages

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
        try:
            changed, deleted = index.get_stale_files(files)
        except IndexNeedsRebuild:
            # Model or format changed - force rebuild
            if not quiet:
                err_console.print("[dim]Index format changed, rebuilding...[/]")
            index.clear()
            build_index(path, quiet=quiet)
            # Skip to cleanup phase
            changed, deleted = [], []
        except RuntimeError as e:
            # Version mismatch or other manifest error
            err_console.print(f"[red]✗[/] {e}")
            # Provide context-aware help if we redirected to a parent
            if path != original_path:
                err_console.print("\nOptions:")
                err_console.print(f"  hhg build --force {path}  # Rebuild parent index")
                err_console.print(
                    f"  hhg build --force {original_path}  # Create separate index in subdir"
                )
            raise typer.Exit(EXIT_ERROR)
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
        merge_info: list[tuple[str, int]] = []
        if subdir_indexes:
            index = SemanticIndex(path)
            for idx in subdir_indexes:
                merge_stats = index.merge_from_subdir(idx)
                merged = merge_stats.get("merged", 0)
                if merged > 0:
                    merged_any = True
                    # Get relative subdir name for display
                    subdir_name = str(idx.parent.relative_to(path))
                    merge_info.append((subdir_name, merged))

        # Build (will skip files already merged via hash matching)
        # Defer summary if we have cleanup to do after
        has_cleanup = bool(subdir_indexes) or merged_any
        result = build_index(
            path,
            quiet=quiet,
            merge_info=merge_info if merge_info else None,
            defer_summary=has_cleanup and not quiet,
        )

        # If we merged, clean up any deleted files from merged manifests
        if merged_any:
            # Reopen index to get fresh state after build
            index = SemanticIndex(path)
            files = scan(str(path), ".", include_hidden=False)
            _changed, deleted = index.get_stale_files(files)
            if deleted:
                index.update(files)
                if not quiet:
                    err_console.print(
                        f"[dim]    Cleaned up: {len(deleted)} deleted file entries[/]"
                    )

        # Clean up subdir indexes (now superseded by parent)
        for idx in subdir_indexes:
            shutil.rmtree(idx)
            if not quiet:
                err_console.print(f"[dim]    Cleaned up: {idx.parent.relative_to(path)}[/]")

        # Print deferred summary after all cleanup
        if result is not None:
            stats, index_time = result
            err_console.print(
                f"  [green]✓[/] Indexed {stats['blocks']} blocks "
                f"from {stats['files']} files ({index_time:.1f}s)"
            )
            if stats.get("errors", 0) > 0:
                err_console.print(f"[yellow]![/] {stats['errors']} files failed to index")
        return

    # Clean up subdir indexes (now superseded by parent) - for non-fresh builds
    for idx in subdir_indexes:
        shutil.rmtree(idx)
        if not quiet:
            err_console.print(f"[dim]    Cleaned up: {idx.parent.relative_to(path)}[/]")


@app.command(name="list")
def list_indexes(path: Path = typer.Argument(Path("."), help="Directory to search")):
    """List all indexes under a directory."""
    from .semantic import IndexNeedsRebuild, SemanticIndex, find_subdir_indexes

    path = path.resolve()
    indexes = find_subdir_indexes(path, include_root=True)

    if not indexes:
        err_console.print("[dim]No indexes found[/]")
        raise typer.Exit()

    for idx_path in indexes:
        idx_root = idx_path.parent
        try:
            rel_path = idx_root.relative_to(path)
            display_path = f"./{rel_path}" if str(rel_path) != "." else "."
        except ValueError:
            display_path = str(idx_root)

        # Get block count from manifest
        index = SemanticIndex(idx_root)
        try:
            block_count = index.count()
            console.print(f"  {display_path}/.hhg/ [dim]({block_count} blocks)[/]")
        except IndexNeedsRebuild:
            console.print(f"  {display_path}/.hhg/ [yellow](needs rebuild)[/]")


@app.command(context_settings={"allow_interspersed_args": True})
def clean(
    path: Path = typer.Argument(Path("."), help="Directory"),
    recursive: bool = typer.Option(
        False, "--recursive", "-r", help="Also delete indexes in subdirectories"
    ),
):
    """Delete index or remove subdir from parent index."""
    import shutil

    from .semantic import (
        IndexNeedsRebuild,
        SemanticIndex,
        find_parent_index,
        find_subdir_indexes,
    )

    path = path.resolve()
    deleted_count = 0

    # Delete root index if exists
    if index_exists(path):
        index = SemanticIndex(path)
        index.clear()
        console.print("[green]✓[/] Deleted ./.hhg/")
        deleted_count += 1
    else:
        # Check if this path is part of a parent index
        parent = find_parent_index(path)
        if parent:
            try:
                rel_prefix = str(path.relative_to(parent))
                # Don't allow cleaning the parent root via subdir path
                if not rel_prefix or rel_prefix == ".":
                    err_console.print(
                        f"[dim]Hint: Use 'hhg clean {parent}' to delete the parent index[/]"
                    )
                else:
                    index = SemanticIndex(parent)
                    try:
                        stats = index.remove_prefix(rel_prefix)
                    except IndexNeedsRebuild:
                        err_console.print(
                            "[yellow]![/] Parent index needs rebuild. "
                            f"Run: hhg build --force {parent}"
                        )
                        raise typer.Exit(EXIT_ERROR)
                    if stats["blocks"] > 0:
                        console.print(
                            f"[green]✓[/] Removed {stats['blocks']} blocks "
                            f"({stats['files']} files) from parent index"
                        )
                        deleted_count += 1
                    else:
                        err_console.print(
                            f"[dim]No blocks found for {rel_prefix} in parent index[/]"
                        )
            except ValueError:
                pass  # path not under parent, shouldn't happen

    # Delete subdir indexes if recursive
    if recursive:
        subdir_indexes = find_subdir_indexes(path, include_root=False)
        for idx_path in subdir_indexes:
            try:
                rel_path = idx_path.parent.relative_to(path)
                shutil.rmtree(idx_path)
                console.print(f"[green]✓[/] Deleted ./{rel_path}/.hhg/")
                deleted_count += 1
            except Exception as e:
                err_console.print(f"[red]✗[/] Failed to delete {idx_path}: {e}")

    if deleted_count == 0:
        err_console.print("[dim]No indexes to delete[/]")
    elif deleted_count > 1:
        console.print(f"[dim]Deleted {deleted_count} indexes[/]")


@app.command()
def model():
    """Show embedding model status."""
    from huggingface_hub import try_to_load_from_cache

    from .embedder import MODEL_REPO, TOKENIZER_FILE, _get_best_provider_and_model, get_embedder

    _, model_file = _get_best_provider_and_model()
    model_cached = try_to_load_from_cache(MODEL_REPO, model_file)
    tokenizer_cached = try_to_load_from_cache(MODEL_REPO, TOKENIZER_FILE)
    is_installed = model_cached is not None and tokenizer_cached is not None

    if is_installed:
        console.print(f"[green]✓[/] Model installed: {MODEL_REPO}")
        # Show provider and batch size
        embedder = get_embedder()
        provider = embedder.provider.replace("ExecutionProvider", "")
        console.print(f"  Provider: {provider} (batch size: {embedder.batch_size})")
        console.print(f"  Model: {model_file}")
    else:
        console.print(f"[yellow]![/] Model not installed: {MODEL_REPO}")
        console.print(f"  Needed: {model_file}")
        console.print("  Run 'hhg model install' to download")


@app.command(name="model-install", hidden=True)
def model_install():
    """Download embedding model (deprecated: use 'hhg model install')."""
    from huggingface_hub import hf_hub_download

    from .embedder import MODEL_REPO, TOKENIZER_FILE, _get_best_provider_and_model

    _, model_file = _get_best_provider_and_model()
    console.print(f"[dim]Downloading {MODEL_REPO} ({model_file})...[/]")

    try:
        for filename in [model_file, TOKENIZER_FILE]:
            hf_hub_download(
                repo_id=MODEL_REPO,
                filename=filename,
                force_download=True,
            )
        console.print(f"[green]✓[/] Model installed: {MODEL_REPO}")
    except Exception as e:
        err_console.print(f"[red]✗[/] Failed to download model: {e}")
        err_console.print("[dim]Check network connection and try again[/]")
        raise typer.Exit(EXIT_ERROR)


_subcommand_original_argv = None


def _parse_subcommand_args(
    typer_path: Path,
    typer_flags: dict[str, bool | str] | None = None,
) -> tuple[Path, dict[str, bool | str]]:
    """Parse subcommand args from saved argv or fall back to typer-parsed values.

    Args:
        typer_path: The path parsed by typer (fallback).
        typer_flags: Dict of flag names to typer-parsed values (fallback).

    Returns:
        Tuple of (path, flags_dict).
    """
    flags = typer_flags or {}

    if not _subcommand_original_argv:
        return typer_path, flags

    args = _subcommand_original_argv[1:]  # Skip subcommand name

    # Parse flags from args
    parsed_flags = {}
    for flag_name, default in flags.items():
        if isinstance(default, bool):
            # Boolean flag - check short and long forms
            short = f"-{flag_name[0]}"
            long = f"--{flag_name}"
            parsed_flags[flag_name] = short in args or long in args
        # String flags would need value parsing (not currently used)

    # Find path (first non-flag arg)
    path = Path(".")
    for arg in args:
        if not arg.startswith("-"):
            path = Path(arg)
            break

    return path, parsed_flags


def _check_help_flag() -> bool:
    """Check if help flag is in saved argv."""
    if not _subcommand_original_argv:
        return False
    args = _subcommand_original_argv[1:]
    return "--help" in args or "-h" in args


def main():
    """Entry point."""
    import sys

    global _subcommand_original_argv

    # Reset global state (important for test isolation)
    _subcommand_original_argv = None

    # Pre-process argv to handle subcommand flags before typer sees them
    # Typer's callback pattern with positional args (query, path) confuses it:
    # "clean . -r" -> query="clean", path=".", leftover "-r" = "command not found"
    # Solution: strip path/flags from subcommands and let callback parse saved argv
    argv = sys.argv[1:]  # Skip program name

    if len(argv) >= 1 and argv[0] in (
        "clean",
        "build",
        "list",
        "status",
        "model",
        "similar",
    ):
        # Save original args for callback to parse
        _subcommand_original_argv = argv
        # Just pass subcommand name to typer
        sys.argv = [sys.argv[0], argv[0]]

    app()


if __name__ == "__main__":
    main()
