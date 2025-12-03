import os
import re
from typing import Any, Dict, List, Optional

import tree_sitter_bash
import tree_sitter_c
import tree_sitter_c_sharp
import tree_sitter_cpp
import tree_sitter_elixir
import tree_sitter_go
import tree_sitter_java
import tree_sitter_javascript
import tree_sitter_json
import tree_sitter_kotlin
import tree_sitter_lua
import tree_sitter_php
import tree_sitter_python
import tree_sitter_ruby
import tree_sitter_rust
import tree_sitter_svelte
import tree_sitter_swift
import tree_sitter_toml
import tree_sitter_typescript
import tree_sitter_yaml
import tree_sitter_zig

try:
    import tree_sitter_mojo

    HAS_MOJO = True
except ImportError:
    HAS_MOJO = False
from tree_sitter import Language, Parser, Query, QueryCursor

LANGUAGE_CAPSULES = {
    ".bash": tree_sitter_bash.language(),
    ".c": tree_sitter_c.language(),
    ".cc": tree_sitter_cpp.language(),
    ".cs": tree_sitter_c_sharp.language(),
    ".cpp": tree_sitter_cpp.language(),
    ".cxx": tree_sitter_cpp.language(),
    ".ex": tree_sitter_elixir.language(),
    ".exs": tree_sitter_elixir.language(),
    ".go": tree_sitter_go.language(),
    ".h": tree_sitter_c.language(),
    ".hh": tree_sitter_cpp.language(),
    ".hpp": tree_sitter_cpp.language(),
    ".java": tree_sitter_java.language(),
    ".js": tree_sitter_javascript.language(),
    ".json": tree_sitter_json.language(),
    ".jsx": tree_sitter_javascript.language(),
    ".kt": tree_sitter_kotlin.language(),
    ".kts": tree_sitter_kotlin.language(),
    ".lua": tree_sitter_lua.language(),
    ".php": tree_sitter_php.language_php(),
    ".py": tree_sitter_python.language(),
    ".rb": tree_sitter_ruby.language(),
    ".rs": tree_sitter_rust.language(),
    ".sh": tree_sitter_bash.language(),
    ".svelte": tree_sitter_svelte.language(),
    ".swift": tree_sitter_swift.language(),
    ".toml": tree_sitter_toml.language(),
    ".ts": tree_sitter_typescript.language_typescript(),
    ".tsx": tree_sitter_typescript.language_tsx(),
    ".yaml": tree_sitter_yaml.language(),
    ".yml": tree_sitter_yaml.language(),
    ".zig": tree_sitter_zig.language(),
    ".zsh": tree_sitter_bash.language(),
}

# Add Mojo if available
if HAS_MOJO:
    LANGUAGE_CAPSULES[".mojo"] = tree_sitter_mojo.language()
    LANGUAGE_CAPSULES[".🔥"] = tree_sitter_mojo.language()

QUERIES = {
    "bash": "(function_definition) @function",
    "c": """
        (function_definition) @function
        (struct_specifier) @class
        (enum_specifier) @class
    """,
    "cpp": """
        (function_definition) @function
        (class_specifier) @class
        (struct_specifier) @class
    """,
    "csharp": """
        (method_declaration) @function
        (constructor_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
        (struct_declaration) @class
    """,
    "elixir": "(call) @function",
    "go": """
        (function_declaration) @function
        (method_declaration) @function
        (type_declaration) @class
    """,
    "java": """
        (method_declaration) @function
        (constructor_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
    """,
    "javascript": """
        (function_declaration) @function
        (class_declaration) @class
        (arrow_function) @function
    """,
    "json": "(pair) @item",
    "kotlin": """
        (function_declaration) @function
        (class_declaration) @class
        (object_declaration) @class
    """,
    "lua": """
        (function_declaration) @function
        (function_definition) @function
    """,
    "mojo": """
        (function_definition) @function
        (class_definition) @class
    """,
    "php": """
        (function_definition) @function
        (method_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
        (trait_declaration) @class
    """,
    "python": """
        (function_definition) @function
        (class_definition) @class
    """,
    "ruby": """
        (method) @function
        (singleton_method) @function
        (class) @class
        (module) @class
    """,
    "rust": """
        (function_item) @function
        (impl_item) @class
        (struct_item) @class
        (trait_item) @class
        (enum_item) @class
    """,
    "svelte": "(script_element) @class",
    "swift": """
        (function_declaration) @function
        (class_declaration) @class
        (protocol_declaration) @class
    """,
    "toml": """
        (table) @item
        (pair) @item
    """,
    "typescript": """
        (function_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
        (arrow_function) @function
    """,
    "yaml": "(block_mapping_pair) @item",
    "zig": """
        (function_declaration) @function
        (struct_declaration) @class
    """,
}


class ContextExtractor:
    def __init__(self):
        self.parsers = {}
        self.languages = {}
        self.queries = {}  # Cache compiled queries per language

        # Pre-initialize parsers and queries
        for ext, capsule in LANGUAGE_CAPSULES.items():
            try:
                # Wrap the PyCapsule in a Language object
                lang = Language(capsule)
                parser = Parser(lang)
                self.parsers[ext] = parser
                self.languages[ext] = lang

                # Pre-compile queries
                lang_name = self._ext_to_lang_name(ext)
                if lang_name and lang_name in QUERIES:
                    self.queries[ext] = Query(lang, QUERIES[lang_name])
            except Exception as e:
                print(f"Warning: Failed to load parser for {ext}: {e}")

    def _ext_to_lang_name(self, ext: str) -> Optional[str]:
        """Map file extension to language name for queries."""
        ext_map = {
            ".bash": "bash",
            ".c": "c",
            ".cc": "cpp",
            ".cpp": "cpp",
            ".cs": "csharp",
            ".cxx": "cpp",
            ".ex": "elixir",
            ".exs": "elixir",
            ".go": "go",
            ".h": "c",
            ".hh": "cpp",
            ".hpp": "cpp",
            ".java": "java",
            ".js": "javascript",
            ".json": "json",
            ".jsx": "javascript",
            ".kt": "kotlin",
            ".kts": "kotlin",
            ".lua": "lua",
            ".mojo": "mojo",
            ".php": "php",
            ".py": "python",
            ".rb": "ruby",
            ".rs": "rust",
            ".sh": "bash",
            ".svelte": "svelte",
            ".swift": "swift",
            ".toml": "toml",
            ".ts": "typescript",
            ".tsx": "typescript",
            ".yaml": "yaml",
            ".yml": "yaml",
            ".zig": "zig",
            ".zsh": "bash",
            ".🔥": "mojo",
        }
        return ext_map.get(ext)

    def _fallback_sliding_window(
        self, file_path: str, content: str, query: str
    ) -> List[Dict[str, Any]]:
        """
        Finds matches of 'query' in 'content' and returns windows +/- 5 lines.
        """
        lines = content.splitlines()
        matches = []
        try:
            # Basic regex search
            for i, line in enumerate(lines):
                if re.search(query, line, re.IGNORECASE):
                    start = max(0, i - 5)
                    end = min(len(lines), i + 6)
                    window = "\n".join(lines[start:end])
                    matches.append(
                        {
                            "type": "text",
                            "name": f"match at line {i + 1}",
                            "start_line": start,
                            "end_line": end,
                            "content": window,
                        }
                    )
                    if len(matches) >= 5:
                        break
        except Exception:
            # If regex fails, return head
            pass

        if matches:
            return matches

        # Return Head (First 50 lines)
        end_head = min(len(lines), 50)
        return [
            {
                "type": "file",
                "name": os.path.basename(file_path),
                "start_line": 0,
                "end_line": end_head,
                "content": "\n".join(lines[:end_head]),
            }
        ]

    def extract(
        self, file_path: str, query: str, content: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        if content is None:
            try:
                with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
                    content = f.read()
            except Exception:
                return []

        _, ext = os.path.splitext(file_path)
        parser = self.parsers.get(ext)

        if not parser:
            return self._fallback_sliding_window(file_path, content, query)

        # Parse
        tree = parser.parse(content.encode())

        # Run Query (use cached query object)
        q_obj = self.queries.get(ext)
        if not q_obj:
            return self._fallback_sliding_window(file_path, content, query)

        captures = []
        try:
            cursor = QueryCursor(q_obj)
            captures = cursor.captures(tree.root_node)
        except Exception as e:
            print(f"Query error for {file_path}: {e}")
            return self._fallback_sliding_window(file_path, content, query)

        blocks = []
        seen_ranges = set()

        iterator = []
        if isinstance(captures, dict):
            for tag_name, nodes in captures.items():
                if not isinstance(nodes, list):
                    nodes = [nodes]
                for n in nodes:
                    iterator.append((n, tag_name))
        else:
            iterator = captures

        for item in iterator:
            node = None
            tag = "unknown"

            if isinstance(item, tuple):
                node = item[0]
                tag = item[1]
            elif hasattr(item, "type"):
                node = item
                tag = "match"
            else:
                continue

            rng = (node.start_byte, node.end_byte)
            if rng in seen_ranges:
                continue
            seen_ranges.add(rng)

            name = "anonymous"
            name_types = (
                "identifier",
                "name",
                "field_identifier",
                "type_identifier",
                "constant",  # Ruby
                "simple_identifier",  # Swift
                "word",  # Bash
            )
            # Search direct children first
            for child in node.children:
                if child.type in name_types:
                    name = child.text.decode("utf8")
                    break
            # If not found, search one level deeper (e.g., Go type_spec)
            if name == "anonymous":
                for child in node.children:
                    for grandchild in child.children:
                        if grandchild.type in name_types:
                            name = grandchild.text.decode("utf8")
                            break
                    if name != "anonymous":
                        break

            blocks.append(
                {
                    "type": tag,
                    "name": name,
                    "start_line": node.start_point[0],
                    "end_line": node.end_point[0],
                    "content": content[node.start_byte : node.end_byte],
                }
            )

        if not blocks:
            return self._fallback_sliding_window(file_path, content, query)

        return blocks
