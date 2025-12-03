import os
import re
from typing import List, Dict, Any, Optional
import tree_sitter_python
import tree_sitter_javascript
import tree_sitter_typescript
import tree_sitter_rust
import tree_sitter_go
import tree_sitter_c
import tree_sitter_cpp
import tree_sitter_java
import tree_sitter_ruby
import tree_sitter_c_sharp
import tree_sitter_mojo
from tree_sitter import Language, Parser, Query, QueryCursor

# Map extensions to language capsules
LANGUAGE_CAPSULES = {
    ".py": tree_sitter_python.language(),
    ".js": tree_sitter_javascript.language(),
    ".jsx": tree_sitter_javascript.language(),
    ".ts": tree_sitter_typescript.language_typescript(),
    ".tsx": tree_sitter_typescript.language_tsx(),
    ".rs": tree_sitter_rust.language(),
    ".go": tree_sitter_go.language(),
    ".c": tree_sitter_c.language(),
    ".h": tree_sitter_c.language(),
    ".cpp": tree_sitter_cpp.language(),
    ".cc": tree_sitter_cpp.language(),
    ".cxx": tree_sitter_cpp.language(),
    ".hpp": tree_sitter_cpp.language(),
    ".hh": tree_sitter_cpp.language(),
    ".java": tree_sitter_java.language(),
    ".rb": tree_sitter_ruby.language(),
    ".cs": tree_sitter_c_sharp.language(),
    ".mojo": tree_sitter_mojo.language(),
    ".🔥": tree_sitter_mojo.language(),  # Mojo's emoji extension
}

QUERIES = {
    "python": """
        (function_definition) @function
        (class_definition) @class
    """,
    "javascript": """
        (function_declaration) @function
        (class_declaration) @class
        (arrow_function) @function
    """,
    "typescript": """
        (function_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
        (arrow_function) @function
    """,
    "rust": """
        (function_item) @function
        (impl_item) @class
        (struct_item) @class
    """,
    "go": """
        (function_declaration) @function
        (method_declaration) @function
        (type_declaration) @class
    """.strip(),
    "mojo": """
        (function_definition) @function
        (class_definition) @class
    """,
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
    "java": """
        (method_declaration) @function
        (constructor_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
    """,
    "ruby": """
        (method) @function
        (singleton_method) @function
        (class) @class
        (module) @class
    """,
    "csharp": """
        (method_declaration) @function
        (constructor_declaration) @function
        (class_declaration) @class
        (interface_declaration) @class
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
        if ext == ".py": return "python"
        if ext in [".js", ".jsx"]: return "javascript"
        if ext in [".ts", ".tsx"]: return "typescript"
        if ext == ".rs": return "rust"
        if ext == ".go": return "go"
        if ext in [".c", ".h"]: return "c"
        if ext in [".cpp", ".cc", ".cxx", ".hpp", ".hh"]: return "cpp"
        if ext == ".java": return "java"
        if ext == ".rb": return "ruby"
        if ext == ".cs": return "csharp"
        if ext in [".mojo", ".🔥"]: return "mojo"
        return None

    def get_language_for_file(self, path: str) -> Optional[Any]:
        _, ext = os.path.splitext(path)
        return self.languages.get(ext)

    def _fallback_sliding_window(self, file_path: str, content: str, query: str) -> List[Dict[str, Any]]:
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
                    matches.append({
                        "type": "text",
                        "name": f"match at line {i+1}",
                        "start_line": start,
                        "end_line": end,
                        "content": window
                    })
                    if len(matches) >= 5: break
        except Exception:
            # If regex fails, return head
            pass
            
        if matches:
            return matches
        
        # Return Head (First 50 lines)
        end_head = min(len(lines), 50)
        return [{
            "type": "file",
            "name": os.path.basename(file_path),
            "start_line": 0,
            "end_line": end_head,
            "content": "\n".join(lines[:end_head])
        }]

    def extract(self, file_path: str, query: str, content: Optional[str] = None) -> List[Dict[str, Any]]:
        if content is None:
            try:
                with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
                    content = f.read()
            except Exception as e:
                return []

        _, ext = os.path.splitext(file_path)
        parser = self.parsers.get(ext)
        
        if not parser:
            return self._fallback_sliding_window(file_path, content, query)

        # Parse
        tree = parser.parse(bytes(content, "utf8"))
        
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
            elif hasattr(item, 'type'): 
                node = item
                tag = "match"
            else:
                continue

            rng = (node.start_byte, node.end_byte)
            if rng in seen_ranges:
                continue
            seen_ranges.add(rng)

            name = "anonymous"
            for child in node.children:
                if child.type == "identifier" or child.type == "name":
                    name = child.text.decode("utf8")
                    break
            
            blocks.append({
                "type": tag,
                "name": name,
                "start_line": node.start_point[0],
                "end_line": node.end_point[0],
                "content": content[node.start_byte:node.end_byte]
            })
            
        if not blocks:
             return self._fallback_sliding_window(file_path, content, query)

        return blocks