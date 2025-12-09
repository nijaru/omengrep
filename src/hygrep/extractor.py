import logging
import os
import re
from typing import Any

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
from tree_sitter import Language, Parser, Query, QueryCursor

try:
    import tree_sitter_mojo

    HAS_MOJO = True
except ImportError:
    HAS_MOJO = False

logger = logging.getLogger(__name__)

# Text/doc file extensions (recursive chunking with context)
TEXT_EXTENSIONS = {".md", ".mdx", ".markdown", ".txt", ".rst"}

# Chunking parameters for prose (based on industry best practices)
# See: https://unstructured.io/blog/chunking-for-rag-best-practices
CHUNK_SIZE = 400  # Target ~400 tokens - industry baseline for balanced retrieval
CHUNK_OVERLAP = 50  # ~50 tokens (~12.5%) overlap for context continuity
MIN_CHUNK_SIZE = 30  # Minimum tokens to create a chunk

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
                logger.debug("Failed to load parser for %s: %s", ext, e)

    def _ext_to_lang_name(self, ext: str) -> str | None:
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

    def _estimate_tokens(self, text: str) -> int:
        """Estimate token count using character-based heuristic.

        ~4 characters per token for English prose (GPT/BERT tokenizers).
        More accurate than word-based estimation for mixed content.
        """
        return max(1, len(text) // 4)

    # Sentence boundary pattern - matches ., !, ? followed by space or end
    # Handles quotes and avoids common abbreviations
    _SENTENCE_PATTERN = re.compile(r'(?<=[.!?])\s+(?=[A-Z"\'])|(?<=[.!?])$')

    def _split_text_recursive(
        self,
        text: str,
        chunk_size: int = CHUNK_SIZE,
        separators: list[str] | None = None,
    ) -> list[str]:
        """Recursively split text into chunks of target size.

        Tries separators in order: paragraphs → lines → sentences → words.
        Uses regex for better sentence boundary detection.
        """
        if separators is None:
            # Use None as sentinel for regex-based sentence splitting
            separators = ["\n\n", "\n", None, " "]  # None = sentence regex

        if self._estimate_tokens(text) <= chunk_size:
            return [text] if text.strip() else []

        # Try each separator
        for i, sep in enumerate(separators):
            # Handle sentence regex (None sentinel)
            if sep is None:
                parts = self._SENTENCE_PATTERN.split(text)
                parts = [p for p in parts if p]  # Remove empty strings
                if len(parts) <= 1:
                    continue
                sep = " "  # Use space as joiner for sentences
            else:
                if sep not in text:
                    continue
                parts = text.split(sep)

            chunks = []
            current = ""

            for part in parts:
                candidate = current + sep + part if current else part
                if self._estimate_tokens(candidate) <= chunk_size:
                    current = candidate
                else:
                    if current:
                        chunks.append(current)
                    # If single part exceeds chunk_size, recurse with finer separator
                    if self._estimate_tokens(part) > chunk_size and i + 1 < len(separators):
                        chunks.extend(
                            self._split_text_recursive(part, chunk_size, separators[i + 1 :])
                        )
                    else:
                        current = part

            if current:
                chunks.append(current)

            if chunks:
                return chunks

        # Fallback: hard split by words
        words = text.split()
        chunks = []
        current_words = []
        for word in words:
            current_words.append(word)
            if self._estimate_tokens(" ".join(current_words)) >= chunk_size:
                chunks.append(" ".join(current_words))
                current_words = []
        if current_words:
            chunks.append(" ".join(current_words))
        return chunks

    def _add_overlap(self, chunks: list[str], overlap: int = CHUNK_OVERLAP) -> list[str]:
        """Add overlap between chunks by prepending end of previous chunk.

        Uses clean overlap without markers to avoid polluting embeddings.
        """
        if len(chunks) <= 1 or overlap <= 0:
            return chunks

        result = [chunks[0]]
        for i in range(1, len(chunks)):
            prev_words = chunks[i - 1].split()
            # Take last N words from previous chunk as overlap
            overlap_words = prev_words[-overlap:] if len(prev_words) > overlap else prev_words
            overlap_text = " ".join(overlap_words)
            # Clean overlap - no markers that would pollute embeddings
            result.append(f"{overlap_text} {chunks[i]}")
        return result

    def _parse_markdown_structure(self, content: str) -> list[dict[str, Any]]:
        """Parse markdown into structured sections with header hierarchy.

        Extracts:
        - Text sections under headers
        - Fenced code blocks as separate searchable items
        """
        lines = content.split("\n")
        sections = []
        current_headers: list[str] = []  # Stack of headers by level
        current_content: list[str] = []
        current_start = 0
        in_code_block = False
        code_block_start = 0
        code_block_lang = None
        code_block_lines: list[str] = []

        def save_current_section(end_line: int):
            """Helper to save accumulated text content."""
            if current_content:
                text = "\n".join(current_content).strip()
                if text:
                    sections.append(
                        {
                            "headers": list(current_headers),
                            "content": text,
                            "start_line": current_start,
                            "end_line": end_line,
                            "type": "text",
                        }
                    )

        for i, line in enumerate(lines):
            # Check for code block fence
            fence_match = re.match(r"^(`{3,}|~{3,})(\w+)?", line)
            if fence_match:
                if not in_code_block:
                    # Starting a code block - save any accumulated text first
                    save_current_section(i - 1)
                    current_content = []

                    in_code_block = True
                    code_block_start = i
                    code_block_lang = fence_match.group(2)  # May be None
                    code_block_lines = []
                else:
                    # Ending a code block - extract it as separate item
                    in_code_block = False
                    if code_block_lines:
                        code_content = "\n".join(code_block_lines)
                        if code_content.strip():
                            sections.append(
                                {
                                    "headers": list(current_headers),
                                    "content": code_content,
                                    "start_line": code_block_start,
                                    "end_line": i,
                                    "type": "code",
                                    "language": code_block_lang,
                                }
                            )
                    current_start = i + 1
                continue

            if in_code_block:
                code_block_lines.append(line)
                continue

            # Check for markdown header (ATX style: # Header)
            header_match = re.match(r"^(#{1,6})\s+(.+)$", line)
            if header_match:
                save_current_section(i - 1)

                # Update header stack
                level = len(header_match.group(1))
                title = header_match.group(2).strip()
                current_headers = current_headers[: level - 1]
                current_headers.append(title)

                current_content = []
                current_start = i
            else:
                current_content.append(line)

        # Don't forget last section
        save_current_section(len(lines) - 1)

        return sections

    def _extract_text_blocks(
        self,
        file_path: str,
        content: str,
    ) -> list[dict[str, Any]]:
        """Extract text blocks from markdown/text files with smart chunking.

        Features:
        - Recursive splitting (paragraph → line → sentence → word)
        - Overlap between chunks for context continuity
        - Header hierarchy context injection for markdown
        """
        blocks = []
        ext = os.path.splitext(file_path)[1].lower()

        # For markdown, use structure-aware parsing
        if ext in {".md", ".mdx", ".markdown"}:
            sections = self._parse_markdown_structure(content)

            for section in sections:
                section_content = section["content"]
                headers = section["headers"]
                context = " > ".join(headers) if headers else None
                section_type = section.get("type", "text")

                # Handle code blocks differently - don't chunk them
                if section_type == "code":
                    lang = section.get("language") or "code"
                    # Prepend language and context for better embeddings
                    code_prefix = f"{context} | {lang}" if context else lang
                    content_with_context = f"{code_prefix}\n{section_content}"

                    blocks.append(
                        {
                            "type": "code",
                            "name": lang,
                            "context": context,
                            "start_line": section["start_line"],
                            "end_line": section["end_line"],
                            "content": content_with_context,
                        }
                    )
                    continue

                # Split text section content into chunks
                chunks = self._split_text_recursive(section_content)
                chunks = self._add_overlap(chunks)

                for chunk in chunks:
                    if self._estimate_tokens(chunk) < MIN_CHUNK_SIZE:
                        continue

                    # Determine block type and name
                    block_type = "section" if headers else "text"
                    name = headers[-1] if headers else None

                    # Prepend context to content for better embeddings
                    content_with_context = f"{context} | {chunk}" if context else chunk

                    blocks.append(
                        {
                            "type": block_type,
                            "name": name,
                            "context": context,
                            "start_line": section["start_line"],
                            "end_line": section["end_line"],
                            "content": content_with_context,
                        }
                    )
        else:
            # For plain text (.txt, .rst), use simple recursive splitting
            chunks = self._split_text_recursive(content)
            chunks = self._add_overlap(chunks)

            line_num = 0
            for chunk in chunks:
                if self._estimate_tokens(chunk) < MIN_CHUNK_SIZE:
                    continue

                # Estimate line numbers
                chunk_lines = chunk.count("\n") + 1
                blocks.append(
                    {
                        "type": "text",
                        "name": None,
                        "start_line": line_num,
                        "end_line": line_num + chunk_lines,
                        "content": chunk,
                    }
                )
                line_num += chunk_lines

        return blocks

    def _fallback_sliding_window(
        self,
        file_path: str,
        content: str,
        query: str,
    ) -> list[dict[str, Any]]:
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
                        },
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
            },
        ]

    def extract(
        self,
        file_path: str,
        query: str,
        content: str | None = None,
    ) -> list[dict[str, Any]]:
        if content is None:
            try:
                with open(file_path, encoding="utf-8", errors="ignore") as f:
                    content = f.read()
            except Exception:
                return []

        _, ext = os.path.splitext(file_path)
        ext_lower = ext.lower()

        # Handle text/doc files with paragraph-based extraction
        if ext_lower in TEXT_EXTENSIONS:
            blocks = self._extract_text_blocks(file_path, content)
            if blocks:
                return blocks
            return self._fallback_sliding_window(file_path, content, query)

        parser = self.parsers.get(ext)

        if not parser:
            return self._fallback_sliding_window(file_path, content, query)

        # Parse - keep encoded bytes for correct byte offset slicing
        content_bytes = content.encode()
        tree = parser.parse(content_bytes)

        # Run Query (use cached query object)
        q_obj = self.queries.get(ext)
        if not q_obj:
            return self._fallback_sliding_window(file_path, content, query)

        captures = []
        try:
            cursor = QueryCursor(q_obj)
            captures = cursor.captures(tree.root_node)
        except Exception as e:
            logger.debug("Query error for %s: %s", file_path, e)
            return self._fallback_sliding_window(file_path, content, query)

        blocks = []
        seen_ranges = set()

        iterator = []
        if isinstance(captures, dict):
            for tag_name, nodes in captures.items():
                node_list = nodes if isinstance(nodes, list) else [nodes]
                iterator.extend((n, tag_name) for n in node_list)
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
                    "content": content_bytes[node.start_byte : node.end_byte].decode(
                        "utf-8", errors="replace"
                    ),
                },
            )

        if not blocks:
            return self._fallback_sliding_window(file_path, content, query)

        return blocks
