from pathlib import Path
from collections import List, Set
from algorithm import parallelize
from memory import UnsafePointer, alloc
from os.path import realpath
from sys import stderr
from src.scanner.c_regex import Regex

fn is_ignored_dir(name: String) -> Bool:
    if name == "node_modules": return True
    if name == "target": return True
    if name == "build": return True
    if name == "dist": return True
    if name == "venv": return True
    if name == "env": return True
    if name == ".git": return True
    if name == ".pixi": return True
    if name == ".vscode": return True
    if name == ".idea": return True
    if name == "__pycache__": return True
    return False

fn is_binary_ext(name: String) -> Bool:
    if name.endswith(".pyc"): return True
    if name.endswith(".o"): return True
    if name.endswith(".so"): return True
    if name.endswith(".dylib"): return True
    if name.endswith(".dll"): return True
    if name.endswith(".bin"): return True
    if name.endswith(".exe"): return True
    if name.endswith(".zip"): return True
    if name.endswith(".tar"): return True
    if name.endswith(".gz"): return True
    if name.endswith(".pdf"): return True
    if name.endswith(".png"): return True
    if name.endswith(".jpg"): return True
    if name.endswith(".jpeg"): return True
    if name.endswith(".gif"): return True
    if name.endswith(".ico"): return True
    if name.endswith(".svg"): return True
    if name.endswith(".lock"): return True
    return False

alias MAX_FILE_SIZE = 1_000_000  # 1MB limit

fn scan_file(file: Path, re: Regex) -> Bool:
    try:
        # Skip files larger than 1MB to avoid OOM
        var stat = file.stat()
        if stat.st_size > MAX_FILE_SIZE:
            return False

        with open(file, "r") as f:
            var content = f.read()
            return re.matches(content)
    except:
        return False

fn hyper_scan(root: Path, pattern: String) raises -> List[Path]:
    var candidates = List[Path]()
    var all_files = List[Path]()
    var visited = Set[String]()  # Track visited dirs to avoid symlink loops

    # 1. Collect files (Single Threaded for now)
    var stack = List[Path]()
    stack.append(root)

    while len(stack) > 0:
        var current = stack.pop()
        if current.is_dir():
            # Check for circular symlinks
            try:
                var real = realpath(current)
                if real in visited:
                    continue  # Already visited this directory
                visited.add(real)
            except:
                pass  # If realpath fails, continue anyway

            try:
                var entries = current.listdir()
                for i in range(len(entries)):
                    var entry = entries[i]
                    var full_path = current / entry

                    # Helper to get name string
                    var name_str = entry.name()

                    if name_str.startswith("."):
                        continue

                    if full_path.is_dir():
                        if is_ignored_dir(name_str):
                            continue
                        stack.append(full_path)
                    else:
                        if is_binary_ext(name_str):
                            continue
                        all_files.append(full_path)
            except:
                print("Error accessing: " + String(current), file=stderr)
                continue
        else:
            all_files.append(current)

    var num_files = len(all_files)
    print("Scanned " + String(num_files) + " files.")

    if num_files == 0:
        return candidates^

    # 2. Parallel Scan
    var re = Regex(pattern)
    var mask = alloc[Bool](num_files)

    # Initialize mask to False (avoid undefined behavior)
    for i in range(num_files):
        mask[i] = False

    @parameter
    fn worker(i: Int):
        if scan_file(all_files[i], re):
            mask[i] = True
        else:
            mask[i] = False

    parallelize[worker](num_files)
    
    # 3. Gather results
    for i in range(num_files):
        if mask[i]:
            candidates.append(all_files[i])
            
    mask.free()
            
    return candidates^