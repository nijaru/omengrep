/// Get tree-sitter query source for a file extension.
pub fn get_query_source(ext: &str) -> Option<&'static str> {
    // Map extension to language name, then get query
    let lang = match ext {
        ".py" => "python",
        ".js" | ".jsx" | ".mjs" => "javascript",
        ".ts" | ".tsx" => "typescript",
        ".rs" => "rust",
        ".go" => "go",
        ".c" | ".h" => "c",
        ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hh" => "cpp",
        ".java" => "java",
        ".rb" => "ruby",
        ".cs" => "csharp",
        ".sh" | ".bash" | ".zsh" => "bash",
        ".php" => "php",
        ".kt" | ".kts" => "kotlin",
        ".lua" => "lua",
        ".swift" => "swift",
        ".ex" | ".exs" => "elixir",
        ".zig" => "zig",
        ".yaml" | ".yml" => "yaml",
        ".toml" => "toml",
        ".json" => "json",
        ".html" | ".htm" => "html",
        ".css" => "css",
        ".hcl" | ".tf" => "hcl",
        ".jl" => "julia",
        _ => return None,
    };
    get_query_for_language(lang)
}

fn get_query_for_language(lang: &str) -> Option<&'static str> {
    Some(match lang {
        "python" => {
            r#"
            (function_definition) @function
            (class_definition) @class
            (decorated_definition) @function
            "#
        }
        "javascript" => {
            r#"
            (function_declaration) @function
            (class_declaration) @class
            (arrow_function) @function
            "#
        }
        "typescript" => {
            r#"
            (function_declaration) @function
            (class_declaration) @class
            (interface_declaration) @class
            (arrow_function) @function
            "#
        }
        "rust" => {
            r#"
            (function_item) @function
            (impl_item) @class
            (struct_item) @class
            (trait_item) @class
            (enum_item) @class
            "#
        }
        "go" => {
            r#"
            (function_declaration) @function
            (method_declaration) @function
            (type_declaration) @class
            "#
        }
        "c" => {
            r#"
            (function_definition) @function
            (struct_specifier) @class
            (enum_specifier) @class
            "#
        }
        "cpp" => {
            r#"
            (function_definition) @function
            (class_specifier) @class
            (struct_specifier) @class
            "#
        }
        "java" => {
            r#"
            (method_declaration) @function
            (constructor_declaration) @function
            (class_declaration) @class
            (interface_declaration) @class
            "#
        }
        "ruby" => {
            r#"
            (method) @function
            (singleton_method) @function
            (class) @class
            (module) @class
            "#
        }
        "csharp" => {
            r#"
            (method_declaration) @function
            (constructor_declaration) @function
            (class_declaration) @class
            (interface_declaration) @class
            (struct_declaration) @class
            "#
        }
        "bash" => "(function_definition) @function",
        "php" => {
            r#"
            (function_definition) @function
            (method_declaration) @function
            (class_declaration) @class
            (interface_declaration) @class
            (trait_declaration) @class
            "#
        }
        "kotlin" => {
            r#"
            (function_declaration) @function
            (class_declaration) @class
            (object_declaration) @class
            "#
        }
        "lua" => {
            r#"
            (function_declaration) @function
            (function_definition) @function
            "#
        }
        "swift" => {
            r#"
            (function_declaration) @function
            (class_declaration) @class
            (protocol_declaration) @class
            "#
        }
        "elixir" => {
            r#"
            (call
              target: (identifier) @_name
              (#match? @_name "^(def|defp|defmacro|defmacrop|defmodule)$")
            ) @function
            "#
        }
        "zig" => {
            r#"
            (function_declaration) @function
            (struct_declaration) @class
            "#
        }
        "yaml" => return None, // fallback_head — too many tiny blocks from tree-sitter
        "toml" => "(table) @item",
        "json" => return None, // fallback_head — JSON isn't semantically searchable
        "html" => {
            r#"
            (element) @element
            (script_element) @script
            (style_element) @style
            "#
        }
        "css" => "(rule_set) @rule",
        "hcl" => "(block) @block",
        "julia" => {
            r#"
            (function_definition) @function
            (struct_definition) @class
            (module_definition) @class
            "#
        }
        "sql" => "(statement) @statement",
        _ => return None,
    })
}
