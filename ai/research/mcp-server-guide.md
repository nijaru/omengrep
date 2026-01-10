# MCP Server Implementation Guide for Claude Code

## Overview

Model Context Protocol (MCP) is an open standard for AI-tool integrations. MCP servers expose tools, resources, and prompts that Claude Code can invoke.

## Dependencies

```toml
# pyproject.toml
dependencies = [
    "mcp[cli]>=1.0.0",  # Official SDK with CLI tools
]
```

Install with:

```bash
uv add "mcp[cli]"
# or
pip install "mcp[cli]"
```

The `mcp` package includes `FastMCP`, a high-level API for building servers.

## Minimal Server Implementation

```python
# server.py
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("hygrep")

@mcp.tool()
def search(query: str, path: str = ".") -> str:
    """Search code using hybrid semantic + keyword matching.

    Args:
        query: Natural language query or code pattern
        path: Directory to search (defaults to current directory)

    Returns:
        JSON string with search results including file paths and snippets
    """
    # Implementation here
    import json
    from hygrep.semantic import SemanticIndex

    index = SemanticIndex(path)
    results = index.search(query, limit=10)
    return json.dumps([{
        "file": r.file,
        "line": r.line,
        "score": r.score,
        "snippet": r.snippet
    } for r in results])

if __name__ == "__main__":
    mcp.run(transport="stdio")
```

## Tool Definition Best Practices

### 1. Use Type Hints and Docstrings

FastMCP automatically generates JSON Schema from type hints and extracts descriptions from docstrings:

```python
@mcp.tool()
def search_code(
    query: str,
    path: str = ".",
    limit: int = 10,
    include_context: bool = True
) -> str:
    """Search codebase using semantic + BM25 hybrid search.

    Args:
        query: Natural language query describing what to find
        path: Root directory to search from
        limit: Maximum number of results to return
        include_context: Whether to include surrounding code context

    Returns:
        JSON array of matches with file, line, score, and snippet
    """
    ...
```

### 2. Return Structured Data as JSON Strings

```python
import json

@mcp.tool()
def find_similar(reference: str) -> str:
    """Find code similar to a reference (file#symbol or file:line).

    Args:
        reference: Reference in format 'file.py#function' or 'file.py:42'

    Returns:
        JSON array of similar code locations
    """
    results = do_search(reference)
    return json.dumps(results, indent=2)
```

### 3. Handle Errors Gracefully

```python
from fastmcp import ToolError

@mcp.tool()
def search(query: str, path: str = ".") -> str:
    """Search code."""
    if not os.path.exists(path):
        raise ToolError(f"Path does not exist: {path}")

    try:
        results = perform_search(query, path)
        return json.dumps(results)
    except IndexNotFoundError:
        raise ToolError(f"No index found. Run 'hhg build {path}' first.")
```

### 4. Async Support

```python
@mcp.tool()
async def search_async(query: str) -> str:
    """Async version for long-running searches."""
    results = await async_search(query)
    return json.dumps(results)
```

### 5. Context for Progress/Logging

```python
from fastmcp import Context

@mcp.tool()
async def build_index(path: str, ctx: Context) -> str:
    """Build search index for a directory.

    Args:
        path: Directory to index
        ctx: MCP context for progress reporting
    """
    files = list_files(path)
    for i, f in enumerate(files):
        ctx.info(f"Indexing {f}")
        await ctx.report_progress(i, len(files))
        index_file(f)

    return json.dumps({"indexed": len(files)})
```

## Claude Code Configuration

### Option 1: CLI Command (Recommended)

```bash
# Add to current project (local scope - default)
claude mcp add hygrep -- uv run /path/to/hygrep/server.py

# Add globally (user scope)
claude mcp add --scope user hygrep -- uv run /path/to/hygrep/server.py

# Add for team (project scope - creates .mcp.json)
claude mcp add --scope project hygrep -- uv run /path/to/hygrep/server.py
```

### Option 2: Direct JSON Configuration

**~/.claude.json** (user/local scope):

```json
{
  "mcpServers": {
    "hygrep": {
      "command": "uv",
      "args": ["run", "/absolute/path/to/server.py"],
      "env": {
        "HHG_AUTO_BUILD": "1"
      }
    }
  }
}
```

**.mcp.json** (project scope - commit to repo):

```json
{
  "mcpServers": {
    "hygrep": {
      "command": "uv",
      "args": ["run", "hygrep-mcp-server"],
      "env": {}
    }
  }
}
```

### Option 3: Using uvx (No Installation)

```json
{
  "mcpServers": {
    "hygrep": {
      "command": "uvx",
      "args": ["hygrep-mcp"]
    }
  }
}
```

## MCP Server Management Commands

```bash
# List all configured servers
claude mcp list

# Get server details
claude mcp get hygrep

# Remove a server
claude mcp remove hygrep

# Import from Claude Desktop
claude mcp import-from-claude-desktop
```

## Transport Options

### stdio (Default - Recommended for CLI)

```python
if __name__ == "__main__":
    mcp.run(transport="stdio")
```

- Used for local servers launched by Claude Code
- No network setup required
- Cannot use `print()` - breaks JSON-RPC protocol

### SSE (Server-Sent Events)

```python
if __name__ == "__main__":
    mcp.run(transport="sse", host="127.0.0.1", port=8050)
```

- For persistent servers
- Supports multiple simultaneous connections
- Configuration uses URL instead of command

### HTTP (Streamable)

```python
if __name__ == "__main__":
    mcp.run(transport="http", host="0.0.0.0", port=8000)
```

- For production deployments
- Supports authentication (OAuth 2.0)

## Gotchas and Best Practices

### 1. Never Print to stdout in stdio Mode

```python
# BAD - breaks JSON-RPC
print("Debug info")

# GOOD - use stderr or logging
import sys
print("Debug info", file=sys.stderr)

# BETTER - use context logging
ctx.info("Debug info")
```

### 2. Use Absolute Paths in Configuration

```json
{
  "command": "uv",
  "args": ["run", "/Users/nick/github/nijaru/hygrep/mcp_server.py"]
}
```

### 3. Handle Index Not Found

```python
@mcp.tool()
def search(query: str, path: str = ".") -> str:
    """Search code."""
    index_path = find_index(path)
    if not index_path:
        raise ToolError(
            f"No .hhg index found in {path} or parent directories. "
            f"Run 'hhg build {path}' first."
        )
    ...
```

### 4. Token Limits

Claude Code warns when MCP output exceeds 10,000 tokens. For large results:

```python
@mcp.tool()
def search(query: str, limit: int = 10) -> str:
    """Search with pagination."""
    # Keep default limit reasonable
    limit = min(limit, 50)  # Cap at 50
    results = do_search(query, limit=limit)
    return json.dumps(results)
```

### 5. Environment Variables

Pass API keys or config via env:

```json
{
  "mcpServers": {
    "hygrep": {
      "command": "uv",
      "args": ["run", "hygrep-mcp"],
      "env": {
        "HHG_MODEL_PATH": "/path/to/model",
        "HHG_AUTO_BUILD": "1"
      }
    }
  }
}
```

### 6. Test with MCP Inspector

```bash
# Run dev server with inspector UI
mcp dev server.py

# Or test directly
uv run server.py
```

## Example: Complete hygrep MCP Server

```python
#!/usr/bin/env python3
"""MCP server for hygrep semantic code search."""

import json
import os
from pathlib import Path

from mcp.server.fastmcp import FastMCP, ToolError

mcp = FastMCP(
    "hygrep",
    instructions="Semantic code search using hybrid vector + BM25."
)


def find_index_root(start_path: str) -> Path | None:
    """Walk up to find .hhg directory."""
    path = Path(start_path).resolve()
    for p in [path] + list(path.parents):
        if (p / ".hhg").exists():
            return p
    return None


@mcp.tool()
def search(query: str, path: str = ".") -> str:
    """Search code using semantic + keyword hybrid matching.

    Args:
        query: Natural language query or code pattern to find
        path: Directory to search (walks up to find index)

    Returns:
        JSON array of results with file, line, score, and snippet
    """
    from hygrep.semantic import SemanticIndex

    abs_path = Path(path).resolve()
    index_root = find_index_root(abs_path)

    if not index_root:
        raise ToolError(
            f"No .hhg index found in {abs_path} or parents. "
            f"Build with: hhg build {abs_path}"
        )

    index = SemanticIndex(index_root)
    results = index.search(query, limit=20, scope=abs_path)

    return json.dumps([
        {
            "file": str(r.file),
            "line": r.line,
            "score": round(r.score, 3),
            "name": r.name,
            "snippet": r.snippet[:500]
        }
        for r in results
    ], indent=2)


@mcp.tool()
def find_similar(reference: str) -> str:
    """Find code similar to a reference location.

    Args:
        reference: Reference as 'file.py#function_name' or 'file.py:42'

    Returns:
        JSON array of similar code locations
    """
    from hygrep.semantic import SemanticIndex

    # Parse reference
    if "#" in reference:
        file_path, symbol = reference.rsplit("#", 1)
        ref_type = "symbol"
    elif ":" in reference:
        file_path, line = reference.rsplit(":", 1)
        ref_type = "line"
        line = int(line)
    else:
        raise ToolError(
            "Reference must be 'file#symbol' or 'file:line'"
        )

    file_path = Path(file_path).resolve()
    if not file_path.exists():
        raise ToolError(f"File not found: {file_path}")

    index_root = find_index_root(file_path.parent)
    if not index_root:
        raise ToolError(f"No index found for {file_path}")

    index = SemanticIndex(index_root)

    if ref_type == "symbol":
        results = index.find_by_symbol(file_path, symbol)
    else:
        results = index.find_by_line(file_path, line)

    return json.dumps([
        {
            "file": str(r.file),
            "line": r.line,
            "score": round(r.score, 3),
            "name": r.name
        }
        for r in results
    ], indent=2)


if __name__ == "__main__":
    mcp.run(transport="stdio")
```

## Reference Servers

- **mcp_server_code_extractor**: Tree-sitter based code extraction
  - https://github.com/ctoth/mcp_server_code_extractor

- **greptile-mcp**: Code search via Greptile API
  - https://github.com/sosacrazy126/greptile-mcp

- **semantic-search-mcp**: Local semantic search
  - https://github.com/mix0z/Semantic-Search-MCP

## Resources

- Official MCP Docs: https://modelcontextprotocol.io/quickstart
- Python SDK: https://github.com/modelcontextprotocol/python-sdk
- Claude Code MCP Docs: https://code.claude.com/docs/en/mcp
- FastMCP Tutorial: https://gofastmcp.com/tutorials/create-mcp-server
