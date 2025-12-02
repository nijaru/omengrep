from src.scanner.walker import hyper_scan
from src.inference.reranker import Reranker
from pathlib import Path
from python import Python, PythonObject
from sys import stderr
import sys

alias VERSION = "0.1.0"

fn print_version():
    print("hygrep " + VERSION)

fn print_help():
    print("hygrep " + VERSION + " - Hybrid search: grep speed + LLM intelligence")
    print("")
    print("Usage: hygrep <query> [path] [options]")
    print("")
    print("Arguments:")
    print("  query       Search query (natural language or regex)")
    print("  path        Directory to search (default: .)")
    print("")
    print("Options:")
    print("  -n N        Number of results (default: 10)")
    print("  --json      Output JSON for agents")
    print("  -q, --quiet Suppress progress messages")
    print("  -h, --help  Show this help")
    print("  -v, --version  Show version")

fn is_regex_pattern(query: String) -> Bool:
    """Check if query contains regex metacharacters."""
    if "*" in query: return True
    if "(" in query: return True
    if "[" in query: return True
    if "\\" in query: return True
    if "|" in query: return True
    if "+" in query: return True
    if "?" in query: return True
    if "^" in query: return True
    if "$" in query: return True
    return False

fn main() raises:
    var args = sys.argv()

    var query = ""
    var path_str = "."
    var json_mode = False
    var quiet_mode = False
    var top_k = 10
    var expect_n = False

    # Arg parsing
    for i in range(1, len(args)):
        var arg = args[i]
        if expect_n:
            top_k = Int(atol(arg))
            expect_n = False
        elif arg == "-h" or arg == "--help":
            print_help()
            return
        elif arg == "-v" or arg == "--version":
            print_version()
            return
        elif arg == "--json":
            json_mode = True
        elif arg == "-q" or arg == "--quiet":
            quiet_mode = True
        elif arg == "-n":
            expect_n = True
        elif arg.startswith("-n"):
            top_k = Int(atol(arg[2:]))
        elif query == "":
            query = arg
        elif path_str == ".":
            path_str = arg

    if query == "":
        if json_mode:
            print("[]")
        else:
            print_help()
        return

    var root = Path(path_str)

    # Validate path
    if not root.exists():
        if json_mode:
            print('{"error": "Path does not exist: ' + path_str + '"}')
        else:
            print("Error: Path does not exist: " + path_str, file=stderr)
        return

    if not root.is_dir():
        if json_mode:
            print('{"error": "Path is not a directory: ' + path_str + '"}')
        else:
            print("Error: Path is not a directory: " + path_str, file=stderr)
        return

    if not json_mode and not quiet_mode:
        print("Searching for '" + query + "' in " + path_str, file=stderr)

    # Query expansion: "login auth" -> "login|auth" for better recall
    var scanner_query = query
    if " " in query and not is_regex_pattern(query):
        var py_query = PythonObject(query)
        scanner_query = String(py_query.replace(" ", "|"))

    # 1. Recall
    var matches = hyper_scan(root, scanner_query)

    if not json_mode and not quiet_mode:
        print("Found " + String(len(matches)) + " candidates", file=stderr)

    if len(matches) == 0:
        if json_mode:
            print("[]")
        return

    # 2. Rerank
    if not json_mode and not quiet_mode:
        print("Reranking...", file=stderr)

    var brain = Reranker()

    if json_mode:
        print(brain.search_raw(query, matches, top_k))
        return

    var results = brain.search(query, matches, top_k)
    var num_results = Int(len(results))

    if num_results == 0:
        print("No relevant results.", file=stderr)
        return

    # Output results
    for i in range(num_results):
        var item = results[i]
        var file = String(item["file"])
        var name = String(item["name"])
        var score = Float64(item["score"])
        var kind = String(item["type"])
        var start_line = String(item["start_line"])

        print(file + ":" + start_line + " [" + kind + "] " + name + " (" + String(score) + ")")
