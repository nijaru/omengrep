from src.scanner.walker import hyper_scan
from src.inference.reranker import Reranker
from pathlib import Path
from python import Python, PythonObject
from sys import stderr
import sys

fn print_help():
    print("hygrep - Hybrid search CLI: grep speed + LLM intelligence")
    print("")
    print("Usage: hygrep <query> [path] [options]")
    print("")
    print("Arguments:")
    print("  query     Search query (natural language or regex)")
    print("  path      Directory to search (default: current directory)")
    print("")
    print("Options:")
    print("  --json      Output structured JSON for agents")
    print("  --top-k N   Number of results to return (default: 10)")
    print("  --help      Show this help message")

fn main() raises:
    var args = sys.argv()

    var query = ""
    var path_str = "."
    var json_mode = False
    var top_k = 10
    var expect_top_k = False

    # Simple manual arg parsing
    # skips args[0] (program name)
    for i in range(1, len(args)):
        var arg = args[i]
        if expect_top_k:
            top_k = Int(atol(arg))
            expect_top_k = False
        elif arg == "--help" or arg == "-h":
            print_help()
            return
        elif arg == "--json":
            json_mode = True
        elif arg == "--top-k":
            expect_top_k = True
        elif arg.startswith("--top-k="):
            top_k = Int(atol(arg[8:]))
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

    # Validate path exists
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
    
    if not json_mode:
        print("HyperGrep: Searching for '" + query + "' in " + path_str)
    
    # --- Recall Strategy ---
    # Transform natural language query into high-recall regex
    var scanner_query = query
    
    # Use Python for string manipulation safety
    var py_query = PythonObject(query)
    
    # Heuristic: If query has spaces but no obvious regex chars, treat as OR
    if " " in query:
        var is_regex = False
        if "*" in query: is_regex = True
        if "(" in query: is_regex = True
        if "[" in query: is_regex = True
        if "\\" in query: is_regex = True
        if "|" in query: is_regex = True
        
        if not is_regex:
            scanner_query = String(py_query.replace(" ", "|"))
    
    # 1. Recall (Scanner)
    var matches = hyper_scan(root, scanner_query)
    
    if not json_mode:
        print("Recall: Found " + String(len(matches)) + " candidates.")
    
    if len(matches) == 0:
        if json_mode:
            print("[]")
        return

    # 2. Smart Search
    if not json_mode:
        print("Analyzing context & reranking...")
        
    var brain = Reranker()
    
    if json_mode:
        var json_res = brain.search_raw(query, matches, top_k)
        print(json_res)
        return

    var results = brain.search(query, matches, top_k)
    var num_results = Int(len(results))
    
    if num_results == 0:
        print("No relevant context found after reranking.")
        return
    
    print("\n--- Top Results ---")
    
    for i in range(num_results):
        var item = results[i]
        var file = String(item["file"])
        var name = String(item["name"])
        var score = Float64(item["score"])
        var kind = String(item["type"])
        var start_line = String(item["start_line"])
        
        print("------------------------------------------------")
        print(file + ":" + start_line)
        print("Symbol: " + kind + " " + name)
        print("Score:  " + String(score))