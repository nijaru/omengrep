"""hygrep CLI - Hybrid grep with neural reranking."""

import argparse
import json
import os
import sys
import time
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib  # Fallback for Python < 3.11

import pathspec

from . import __version__
from .reranker import _models_valid, get_execution_providers, MODEL_REPO

# Config file location
CONFIG_PATH = Path.home() / ".config" / "hygrep" / "config.toml"


def load_config() -> dict:
    """Load config from ~/.config/hygrep/config.toml if it exists."""
    if not CONFIG_PATH.exists():
        return {}
    try:
        with open(CONFIG_PATH, "rb") as f:
            return tomllib.load(f)
    except Exception:
        return {}

# Exit codes (grep convention)
EXIT_MATCH = 0
EXIT_NO_MATCH = 1
EXIT_ERROR = 2


def show_info():
    """Show system info and verify installation."""
    print(f"hygrep {__version__}")
    print()

    # Check models
    model_path = "models/reranker.onnx"
    tokenizer_path = "models/tokenizer.json"
    if _models_valid(model_path, tokenizer_path):
        size_mb = os.path.getsize(model_path) / 1024 / 1024
        print(f"Models:    OK ({size_mb:.0f}MB)")
    else:
        print(f"Models:    Not installed (run any search to download)")

    # Check device/provider
    providers = get_execution_providers()
    provider = providers[0].replace("ExecutionProvider", "")
    all_providers = ", ".join(p.replace("ExecutionProvider", "") for p in providers)
    print(f"Device:    {provider}")
    if len(providers) > 1:
        print(f"           Available: {all_providers}")

    # Check scanner
    try:
        from ._scanner import scan
        print("Scanner:   OK (Mojo native)")
    except ImportError:
        print("Scanner:   OK (Python fallback)")

    # Check tree-sitter languages
    try:
        from .extractor import LANGUAGE_CAPSULES
        langs = [ext.lstrip(".") for ext in LANGUAGE_CAPSULES.keys() if not ext.startswith(".🔥")]
        print(f"Languages: {', '.join(langs)}")
    except ImportError:
        print("Languages: Error loading tree-sitter")

    # Config file
    if CONFIG_PATH.exists():
        print(f"Config:    {CONFIG_PATH}")
    else:
        print(f"Config:    (none)")

    print()
    print(f"Model: {MODEL_REPO}")

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


BASH_COMPLETION = '''
_hygrep() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="-n -t -C -q -v -h --json --fast --quiet --version --help --type --context --max-candidates --color --no-ignore --stats --min-score --exclude --hidden"

    case "${prev}" in
        -t|--type)
            COMPREPLY=( $(compgen -W "py js ts rust go mojo java c cpp cs rb php sh md json yaml toml" -- ${cur}) )
            return 0
            ;;
        --color)
            COMPREPLY=( $(compgen -W "auto always never" -- ${cur}) )
            return 0
            ;;
        -n|-C|--max-candidates|--min-score)
            return 0
            ;;
    esac

    if [[ ${cur} == -* ]] ; then
        COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
        return 0
    fi

    COMPREPLY=( $(compgen -d -- ${cur}) )
}
complete -F _hygrep hygrep
'''

ZSH_COMPLETION = '''
#compdef hygrep

_hygrep() {
    local -a opts
    opts=(
        '-n[Number of results]:count:'
        '-t[Filter by file type]:type:(py js ts rust go mojo java c cpp cs rb php sh md json yaml toml)'
        '--type[Filter by file type]:type:(py js ts rust go mojo java c cpp cs rb php sh md json yaml toml)'
        '-C[Show N lines of context]:lines:'
        '--context[Show N lines of context]:lines:'
        '-q[Suppress progress messages]'
        '--quiet[Suppress progress messages]'
        '-v[Show version]'
        '--version[Show version]'
        '-h[Show help]'
        '--help[Show help]'
        '--json[Output JSON for agents]'
        '--fast[Skip neural reranking]'
        '--max-candidates[Max candidates to rerank]:count:'
        '--color[Color output]:when:(auto always never)'
        '--no-ignore[Ignore .gitignore files]'
        '--stats[Show timing statistics]'
        '--min-score[Filter results below score]:score:'
        '--exclude[Exclude files matching pattern]:pattern:'
        '--hidden[Include hidden files and directories]'
    )
    _arguments -s $opts '*:directory:_files -/'
}

_hygrep "$@"
'''

FISH_COMPLETION = '''
complete -c hygrep -s n -d "Number of results"
complete -c hygrep -s t -l type -d "Filter by file type" -xa "py js ts rust go mojo java c cpp cs rb php sh md json yaml toml"
complete -c hygrep -s C -l context -d "Show N lines of context"
complete -c hygrep -s q -l quiet -d "Suppress progress messages"
complete -c hygrep -s v -l version -d "Show version"
complete -c hygrep -s h -l help -d "Show help"
complete -c hygrep -l json -d "Output JSON for agents"
complete -c hygrep -l fast -d "Skip neural reranking"
complete -c hygrep -l max-candidates -d "Max candidates to rerank"
complete -c hygrep -l color -d "Color output" -xa "auto always never"
complete -c hygrep -l no-ignore -d "Ignore .gitignore files"
complete -c hygrep -l stats -d "Show timing statistics"
complete -c hygrep -l min-score -d "Filter results below score"
complete -c hygrep -l exclude -d "Exclude files matching pattern"
complete -c hygrep -l hidden -d "Include hidden files and directories"
'''


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
    parser.add_argument(
        "--stats", action="store_true",
        help="Show timing statistics"
    )
    parser.add_argument(
        "--min-score", type=float, default=0.0, metavar="N",
        help="Filter results below this score threshold"
    )
    parser.add_argument(
        "--exclude", action="append", default=[], metavar="PATTERN",
        help="Exclude files matching glob pattern (can be repeated)"
    )
    parser.add_argument(
        "--completions", choices=["bash", "zsh", "fish"],
        help="Output shell completion script and exit"
    )
    parser.add_argument(
        "--hidden", action="store_true",
        help="Include hidden files and directories"
    )

    args = parser.parse_args()

    # Load config and apply defaults (CLI args override config)
    config = load_config()
    if config:
        # Apply config defaults for unset args
        if args.n == 10 and "n" in config:  # 10 is argparse default
            args.n = config["n"]
        if args.max_candidates == 100 and "max_candidates" in config:
            args.max_candidates = config["max_candidates"]
        if args.color == "auto" and "color" in config:
            args.color = config["color"]
        if args.min_score == 0.0 and "min_score" in config:
            args.min_score = config["min_score"]
        if not args.fast and config.get("fast", False):
            args.fast = True
        if not args.quiet and config.get("quiet", False):
            args.quiet = True
        if not args.hidden and config.get("hidden", False):
            args.hidden = True
        if not args.no_ignore and config.get("no_ignore", False):
            args.no_ignore = True
        if not args.exclude and "exclude" in config:
            args.exclude = config["exclude"] if isinstance(config["exclude"], list) else [config["exclude"]]

    # Handle completions
    if args.completions:
        if args.completions == "bash":
            print(BASH_COMPLETION.strip())
        elif args.completions == "zsh":
            print(ZSH_COMPLETION.strip())
        elif args.completions == "fish":
            print(FISH_COMPLETION.strip())
        sys.exit(0)

    # Determine color usage
    if args.color == "always":
        color = True
    elif args.color == "never":
        color = False
    else:
        color = use_color() and not args.json

    # Handle 'hygrep info' command
    if args.query == "info":
        show_info()
        sys.exit(EXIT_MATCH)

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

    # Stats tracking
    stats = {"scan_ms": 0, "filter_ms": 0, "rerank_ms": 0, "total_ms": 0}
    total_start = time.perf_counter()

    # Query expansion: "login auth" -> "login|auth" for better recall
    scanner_query = args.query
    if " " in args.query and not is_regex_pattern(args.query):
        scanner_query = args.query.replace(" ", "|")

    # 1. Recall phase - Try Mojo scanner, fall back to Python
    try:
        from ._scanner import scan
    except ImportError:
        from .scanner import scan

    scan_start = time.perf_counter()
    file_contents = scan(str(root), scanner_query, args.hidden)
    stats["scan_ms"] = int((time.perf_counter() - scan_start) * 1000)

    filter_start = time.perf_counter()

    # Filter by gitignore (unless --no-ignore)
    if not args.no_ignore:
        gitignore_spec = load_gitignore(root)
        if gitignore_spec:
            # Scanner returns paths relative to root or with ./ prefix
            file_contents = {
                k: v for k, v in file_contents.items()
                if not gitignore_spec.match_file(k.lstrip("./"))
            }

    # Filter by exclude patterns
    if args.exclude:
        exclude_spec = pathspec.PathSpec.from_lines("gitwildmatch", args.exclude)
        file_contents = {
            k: v for k, v in file_contents.items()
            if not exclude_spec.match_file(k.lstrip("./"))
        }

    # Filter by file type if specified
    if args.file_types:
        type_map = {
            "py": [".py"],
            "js": [".js", ".jsx"],
            "ts": [".ts", ".tsx"],
            "rust": [".rs"],
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
        for t in args.file_types.split(","):
            t = t.strip().lower()
            if t in type_map:
                allowed_exts.update(type_map[t])
            else:
                allowed_exts.add(f".{t}")
        file_contents = {
            k: v for k, v in file_contents.items() if any(k.endswith(ext) for ext in allowed_exts)
        }

    stats["filter_ms"] = int((time.perf_counter() - filter_start) * 1000)

    if not args.quiet and not args.json:
        print(f"Found {len(file_contents)} candidates", file=sys.stderr)

    if len(file_contents) == 0:
        if args.json:
            print("[]")
        sys.exit(EXIT_NO_MATCH)

    # 2. Rerank phase (or fast mode)
    rerank_start = time.perf_counter()
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

    stats["rerank_ms"] = int((time.perf_counter() - rerank_start) * 1000)
    stats["total_ms"] = int((time.perf_counter() - total_start) * 1000)

    # Filter by min-score (only in rerank mode)
    if args.min_score > 0 and not args.fast:
        results = [r for r in results if r["score"] >= args.min_score]

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

    # Show stats if requested
    if args.stats:
        print(f"\n{c.BOLD}Stats:{c.RESET}", file=sys.stderr)
        print(f"  Scan:   {stats['scan_ms']:>5}ms", file=sys.stderr)
        print(f"  Filter: {stats['filter_ms']:>5}ms", file=sys.stderr)
        print(f"  Rerank: {stats['rerank_ms']:>5}ms", file=sys.stderr)
        print(f"  Total:  {stats['total_ms']:>5}ms", file=sys.stderr)
        if not args.fast and 'reranker' in dir():
            provider = reranker.provider.replace('ExecutionProvider', '')
            print(f"  Device: {provider}", file=sys.stderr)

    sys.exit(EXIT_MATCH)


if __name__ == "__main__":
    main()
