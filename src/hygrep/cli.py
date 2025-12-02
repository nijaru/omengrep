"""hygrep CLI - Hybrid grep with neural reranking."""

import argparse
import json
import os
import sys
from pathlib import Path

import pathspec

from . import __version__

# Exit codes (grep convention)
EXIT_MATCH = 0
EXIT_NO_MATCH = 1
EXIT_ERROR = 2

# ANSI color codes
class Colors:
    CYAN = "\033[36m"
    YELLOW = "\033[33m"
    GREEN = "\033[32m"
    MAGENTA = "\033[35m"
    RESET = "\033[0m"
    BOLD = "\033[1m"


def use_color() -> bool:
    """Check if we should use color output."""
    # NO_COLOR standard: https://no-color.org/
    if os.environ.get("NO_COLOR"):
        return False
    # Check if stdout is a TTY
    if not sys.stdout.isatty():
        return False
    return True


def load_gitignore(root: Path) -> pathspec.PathSpec | None:
    """Load .gitignore patterns from root directory and parents."""
    patterns = []

    # Walk up to find git root
    current = root.resolve()
    while current != current.parent:
        gitignore = current / ".gitignore"
        if gitignore.exists():
            try:
                patterns.extend(gitignore.read_text().splitlines())
            except (OSError, UnicodeDecodeError):
                pass
        # Stop at git root
        if (current / ".git").exists():
            break
        current = current.parent

    if not patterns:
        return None

    return pathspec.PathSpec.from_lines("gitwildmatch", patterns)


def is_regex_pattern(query: str) -> bool:
    """Check if query contains regex metacharacters."""
    return any(c in query for c in "*()[]\\|+?^$")


def main():
    parser = argparse.ArgumentParser(
        prog="hygrep",
        description="Hybrid search: grep speed + LLM intelligence",
    )
    parser.add_argument("query", nargs="?", help="Search query (natural language or regex)")
    parser.add_argument("path", nargs="?", default=".", help="Directory to search (default: .)")
    parser.add_argument("-n", type=int, default=10, help="Number of results (default: 10)")
    parser.add_argument("--json", action="store_true", help="Output JSON for agents")
    parser.add_argument("-q", "--quiet", action="store_true", help="Suppress progress messages")
    parser.add_argument("-v", "--version", action="version", version=f"hygrep {__version__}")
    parser.add_argument("--fast", action="store_true", help="Skip neural reranking (instant grep)")
    parser.add_argument(
        "-t", "--type", dest="file_types", help="Filter by file type (e.g., py,js,ts)"
    )
    parser.add_argument(
        "--max-candidates", type=int, default=100, help="Max candidates to rerank (default: 100)"
    )
    parser.add_argument(
        "--color", choices=["auto", "always", "never"], default="auto",
        help="Color output (default: auto)"
    )
    parser.add_argument(
        "--no-ignore", action="store_true",
        help="Don't respect .gitignore files"
    )
    parser.add_argument(
        "-C", "--context", type=int, default=0, metavar="N",
        help="Show N lines of context from matched content"
    )

    args = parser.parse_args()

    # Determine color usage
    if args.color == "always":
        color = True
    elif args.color == "never":
        color = False
    else:
        color = use_color() and not args.json

    if not args.query:
        if args.json:
            print("[]")
        else:
            parser.print_help()
        sys.exit(EXIT_NO_MATCH)

    root = Path(args.path)
    if not root.exists():
        if args.json:
            print(json.dumps({"error": f"Path does not exist: {args.path}"}))
        else:
            print(f"Error: Path does not exist: {args.path}", file=sys.stderr)
        sys.exit(EXIT_ERROR)

    if not root.is_dir():
        if args.json:
            print(json.dumps({"error": f"Path is not a directory: {args.path}"}))
        else:
            print(f"Error: Path is not a directory: {args.path}", file=sys.stderr)
        sys.exit(EXIT_ERROR)

    if not args.quiet and not args.json:
        print(f"Searching for '{args.query}' in {args.path}", file=sys.stderr)

    # Query expansion: "login auth" -> "login|auth" for better recall
    scanner_query = args.query
    if " " in args.query and not is_regex_pattern(args.query):
        scanner_query = args.query.replace(" ", "|")

    # 1. Recall phase - Mojo scanner
    try:
        from ._scanner import scan
    except ImportError:
        print("Error: _scanner.so not found. Run: mojo build src/scanner/_scanner.mojo --emit shared-lib -o src/hygrep/_scanner.so", file=sys.stderr)
        sys.exit(EXIT_ERROR)

    file_contents = scan(str(root), scanner_query)

    # Filter by gitignore (unless --no-ignore)
    if not args.no_ignore:
        gitignore_spec = load_gitignore(root)
        if gitignore_spec:
            # Scanner returns paths relative to root or with ./ prefix
            file_contents = {
                k: v for k, v in file_contents.items()
                if not gitignore_spec.match_file(k.lstrip("./"))
            }

    # Filter by file type if specified
    if args.file_types:
        type_map = {
            "py": [".py"],
            "js": [".js", ".jsx"],
            "ts": [".ts", ".tsx"],
            "rust": [".rs"],
            "go": [".go"],
            "java": [".java"],
            "c": [".c", ".h"],
            "cpp": [".cpp", ".cc", ".cxx", ".hpp", ".hh"],
            "rb": [".rb"],
            "php": [".php"],
            "sh": [".sh", ".bash"],
            "md": [".md", ".markdown"],
            "json": [".json"],
            "yaml": [".yaml", ".yml"],
            "toml": [".toml"],
        }
        allowed_exts = set()
        for t in args.file_types.split(","):
            t = t.strip().lower()
            if t in type_map:
                allowed_exts.update(type_map[t])
            else:
                allowed_exts.add(f".{t}")
        file_contents = {
            k: v for k, v in file_contents.items() if any(k.endswith(ext) for ext in allowed_exts)
        }

    if not args.quiet and not args.json:
        print(f"Found {len(file_contents)} candidates", file=sys.stderr)

    if len(file_contents) == 0:
        if args.json:
            print("[]")
        sys.exit(EXIT_NO_MATCH)

    # 2. Rerank phase (or fast mode)
    if args.fast:
        # Fast mode: skip neural reranking, just return grep matches with extraction
        from .extractor import ContextExtractor

        extractor = ContextExtractor()
        results = []
        for path, content in list(file_contents.items())[: args.n]:
            blocks = extractor.extract(path, args.query, content=content)
            for block in blocks:
                results.append({
                    "file": path,
                    "type": block["type"],
                    "name": block["name"],
                    "start_line": block["start_line"],
                    "content": block["content"],
                    "score": 0.0,  # No score in fast mode
                })
        results = results[: args.n]
    else:
        if not args.quiet and not args.json:
            print("Reranking...", file=sys.stderr)

        from .reranker import Reranker

        reranker = Reranker()
        results = reranker.search(
            args.query, file_contents, top_k=args.n, max_candidates=args.max_candidates
        )

    if args.json:
        print(json.dumps(results))
        sys.exit(EXIT_MATCH if results else EXIT_NO_MATCH)

    if not results:
        print("No relevant results.", file=sys.stderr)
        sys.exit(EXIT_NO_MATCH)

    # Output results with optional color
    c = Colors if color else type("NoColor", (), {k: "" for k in dir(Colors) if not k.startswith("_")})()
    for item in results:
        file = item["file"]
        name = item["name"]
        score = item["score"]
        kind = item["type"]
        start_line = item["start_line"]
        content = item.get("content", "")

        # Format: path:line [type] name (score)
        path_str = f"{c.CYAN}{file}{c.RESET}:{c.GREEN}{start_line}{c.RESET}"
        type_str = f"{c.YELLOW}[{kind}]{c.RESET}"
        name_str = f"{c.BOLD}{name}{c.RESET}"

        if args.fast:
            print(f"{path_str} {type_str} {name_str}")
        else:
            score_str = f"{c.MAGENTA}({score:.2f}){c.RESET}"
            print(f"{path_str} {type_str} {name_str} {score_str}")

        # Show context if requested
        if args.context > 0 and content:
            lines = content.splitlines()
            context_lines = lines[:args.context]
            for i, line in enumerate(context_lines):
                line_num = start_line + i
                print(f"  {c.GREEN}{line_num:4d}{c.RESET} | {line}")
            if len(lines) > args.context:
                print(f"  {c.MAGENTA}... ({len(lines) - args.context} more lines){c.RESET}")
            print()  # Blank line between results

    sys.exit(EXIT_MATCH)


if __name__ == "__main__":
    main()
