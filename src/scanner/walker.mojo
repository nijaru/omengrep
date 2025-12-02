from pathlib import Path
from collections import List, Set, Dict
from algorithm import parallelize
from memory import UnsafePointer, alloc
from os.path import realpath
from sys import stderr
from src.scanner.c_regex import Regex


@fieldwise_init
struct ScanMatch(Stringable, Copyable, Movable):
    """A file that matched the search pattern, with its content."""
    var path: Path
    var content: String

    fn __str__(self) -> String:
        return String(self.path)

    fn __copyinit__(out self, existing: Self):
        self.path = existing.path
        self.content = existing.content

    fn __moveinit__(out self, deinit existing: Self):
        self.path = existing.path^
        self.content = existing.content^

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


fn scan_file_with_content(file: Path, re: Regex) -> String:
    """Returns file content if matches, empty string if not."""
    try:
        var stat = file.stat()
        if stat.st_size > MAX_FILE_SIZE:
            return ""

        with open(file, "r") as f:
            var content = f.read()
            if re.matches(content):
                return content
            return ""
    except:
        return ""

fn hyper_scan(root: Path, pattern: String) raises -> List[ScanMatch]:
    var candidates = List[ScanMatch]()
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
    print("Scanned " + String(num_files) + " files.", file=stderr)

    if num_files == 0:
        return candidates^

    # 2. Parallel Scan - store content for matching files
    var re = Regex(pattern)
    var mask = alloc[Bool](num_files)

    # Pre-allocate content storage (empty strings)
    var contents = List[String](capacity=num_files)
    for i in range(num_files):
        mask[i] = False
        contents.append("")

    @parameter
    fn worker(i: Int):
        var result = scan_file_with_content(all_files[i], re)
        if len(result) > 0:
            mask[i] = True
            contents[i] = result
        else:
            mask[i] = False

    parallelize[worker](num_files)

    # 3. Gather results with content
    for i in range(num_files):
        if mask[i]:
            candidates.append(ScanMatch(all_files[i], contents[i]))

    mask.free()

    return candidates^