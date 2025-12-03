"""Smoke tests for all supported languages.

Verifies tree-sitter parsing and query extraction work for each language.
"""

import sys

sys.path.insert(0, "src")

from hygrep.extractor import ContextExtractor

SAMPLES = {
    ".py": ("def hello(): pass\nclass Foo: pass", ["hello", "Foo"]),
    ".js": ("function hello() {}\nclass Foo {}", ["hello", "Foo"]),
    ".ts": ("function hello(): void {}\nclass Foo {}", ["hello", "Foo"]),
    ".tsx": ("function Hello(): JSX.Element { return <div/> }", ["Hello"]),
    ".rs": ("fn hello() {}\nstruct Foo {}", ["hello", "Foo"]),
    ".go": ("func hello() {}\ntype Foo struct {}", ["hello", "Foo"]),
    ".c": ("void hello() {}\nstruct Foo {};", ["hello", "Foo"]),
    ".cpp": ("void hello() {}\nclass Foo {};", ["hello", "Foo"]),
    ".java": ("class Foo { void hello() {} }", ["hello", "Foo"]),
    ".rb": ("def hello; end\nclass Foo; end", ["hello", "Foo"]),
    ".cs": ("class Foo { void Hello() {} }", ["Hello", "Foo"]),
    ".sh": ("hello() { echo hi; }", ["hello"]),
    ".php": ("<?php function hello() {} class Foo {}", ["hello", "Foo"]),
    ".kt": ("fun hello() {}\nclass Foo {}", ["hello", "Foo"]),
    ".lua": ("function hello() end", ["hello"]),
    ".swift": ("func hello() {}\nclass Foo {}", ["hello", "Foo"]),
    ".ex": ("def hello do end", []),  # Elixir uses (call), names hard to extract
    ".zig": ("fn hello() void {}", ["hello"]),
    ".svelte": ("<script>let x = 1;</script>", []),  # Script element, no name
    ".yaml": ("database:\n  host: localhost", []),  # Pairs, no named functions
    ".toml": ('[database]\nhost = "localhost"', []),  # Tables/pairs
    ".json": ('{"name": "test"}', []),  # Pairs, no named functions
}


def test_all_languages_parse():
    """Verify all languages parse without errors."""
    extractor = ContextExtractor()
    results = {}

    for ext, (code, expected_names) in SAMPLES.items():
        blocks = extractor.extract(f"test{ext}", "test", code)
        results[ext] = blocks

        # Should return something (blocks or fallback)
        assert blocks is not None, f"{ext}: extract returned None"
        assert isinstance(blocks, list), f"{ext}: extract didn't return list"

        # For code languages, verify we got expected blocks
        if expected_names:
            found_names = [b.get("name", "") for b in blocks]
            for name in expected_names:
                assert name in found_names, f"{ext}: missing '{name}', got {found_names}"

    print(f"\n{'=' * 60}")
    print("Language extraction summary:")
    print(f"{'=' * 60}")
    for ext, blocks in sorted(results.items()):
        names = [b.get("name", "?") for b in blocks[:3]]
        print(f"  {ext:8} → {len(blocks)} blocks: {names}")
    print(f"{'=' * 60}")
    print(f"Total: {len(results)} languages tested")


def test_extraction_has_content():
    """Verify extracted blocks have actual content."""
    extractor = ContextExtractor()

    # Test a few key languages with longer samples
    samples = {
        ".py": '''
def calculate_total(items):
    """Calculate the total price of items."""
    return sum(item.price for item in items)

class ShoppingCart:
    def __init__(self):
        self.items = []
''',
        ".rs": """
fn process_data(input: &str) -> Result<Data, Error> {
    let parsed = parse(input)?;
    Ok(transform(parsed))
}

struct DataProcessor {
    cache: HashMap<String, Data>,
}
""",
        ".go": """
func HandleRequest(w http.ResponseWriter, r *http.Request) {
    data := parseRequest(r)
    respond(w, data)
}

type Server struct {
    addr string
    port int
}
""",
    }

    for ext, code in samples.items():
        blocks = extractor.extract(f"test{ext}", "test", code)

        for block in blocks:
            # Each block should have content
            assert "content" in block, f"{ext}: block missing content"
            assert len(block["content"]) > 5, f"{ext}: block content too short"

            # Each block should have line numbers
            assert "start_line" in block, f"{ext}: block missing start_line"
            assert "end_line" in block, f"{ext}: block missing end_line"


if __name__ == "__main__":
    print("Running language smoke tests...")
    test_all_languages_parse()
    print("\nRunning content extraction tests...")
    test_extraction_has_content()
    print("\nAll tests passed!")
