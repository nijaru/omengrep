use tree_sitter::Language;

/// Get tree-sitter Language for a file extension.
pub fn get_language(ext: &str) -> Option<Language> {
    match ext {
        ".py" => Some(tree_sitter_python::LANGUAGE.into()),
        ".js" | ".jsx" | ".mjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        ".ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        ".tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        ".rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        ".go" => Some(tree_sitter_go::LANGUAGE.into()),
        ".c" | ".h" => Some(tree_sitter_c::LANGUAGE.into()),
        ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hh" => Some(tree_sitter_cpp::LANGUAGE.into()),
        ".java" => Some(tree_sitter_java::LANGUAGE.into()),
        ".rb" => Some(tree_sitter_ruby::LANGUAGE.into()),
        ".cs" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        ".sh" | ".bash" | ".zsh" => Some(tree_sitter_bash::LANGUAGE.into()),
        ".php" => Some(tree_sitter_php::LANGUAGE_PHP.into()),
        ".kt" | ".kts" => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        ".lua" => Some(tree_sitter_lua::LANGUAGE.into()),
        ".swift" => Some(tree_sitter_swift::LANGUAGE.into()),
        ".ex" | ".exs" => Some(tree_sitter_elixir::LANGUAGE.into()),
        ".zig" => Some(tree_sitter_zig::LANGUAGE.into()),
        ".yaml" | ".yml" => Some(tree_sitter_yaml::LANGUAGE.into()),
        ".toml" => Some(tree_sitter_toml_ng::LANGUAGE.into()),
        ".json" => Some(tree_sitter_json::LANGUAGE.into()),
        ".html" | ".htm" => Some(tree_sitter_html::LANGUAGE.into()),
        ".css" => Some(tree_sitter_css::LANGUAGE.into()),
        ".hcl" | ".tf" => Some(tree_sitter_hcl::LANGUAGE.into()),
        _ => None,
    }
}
