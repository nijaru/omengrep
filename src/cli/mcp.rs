use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::boost::boost_results;
use crate::embedder;
use crate::index::manifest::Manifest;
use crate::index::{self, walker, SemanticIndex, INDEX_DIR};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn run() -> Result<()> {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));

        // Notifications (no id) don't get a response
        if id.is_none() {
            continue;
        }

        let response = match method {
            "initialize" => handle_initialize(),
            "tools/list" => handle_tools_list(),
            "tools/call" => handle_tools_call(&params),
            _ => Err(json_rpc_error(-32601, "Method not found")),
        };

        let reply = match response {
            Ok(result) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }),
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": error,
            }),
        };

        let out = serde_json::to_string(&reply)?;
        writeln!(stdout, "{out}")?;
        stdout.flush()?;
    }

    Ok(())
}

fn json_rpc_error(code: i64, message: &str) -> Value {
    json!({
        "code": code,
        "message": message,
    })
}

fn handle_initialize() -> Result<Value, Value> {
    Ok(json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "omengrep",
            "version": env!("CARGO_PKG_VERSION"),
        }
    }))
}

fn handle_tools_list() -> Result<Value, Value> {
    Ok(json!({
        "tools": [
            {
                "name": "og_search",
                "description": "Semantic code search. Returns code blocks matching the query, ranked by relevance using multi-vector ColBERT embeddings + BM25 hybrid scoring.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (natural language or code pattern)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search (default: current directory)"
                        },
                        "num_results": {
                            "type": "integer",
                            "description": "Number of results to return (default: 10)"
                        }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "og_similar",
                "description": "Find code blocks similar to a given file, function, or line. Use file#name for functions or file:line for specific lines.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "reference": {
                            "type": "string",
                            "description": "File reference: file#function_name, file:line_number, or file path"
                        },
                        "num_results": {
                            "type": "integer",
                            "description": "Number of results to return (default: 10)"
                        }
                    },
                    "required": ["reference"]
                }
            },
            {
                "name": "og_status",
                "description": "Show index status for a directory: number of indexed files, blocks, and model used.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory to check (default: current directory)"
                        }
                    }
                }
            }
        ]
    }))
}

fn handle_tools_call(params: &Value) -> Result<Value, Value> {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool_name {
        "og_search" => tool_search(&args),
        "og_similar" => tool_similar(&args),
        "og_status" => tool_status(&args),
        _ => Err(json_rpc_error(
            -32602,
            &format!("Unknown tool: {tool_name}"),
        )),
    }
}

fn tool_search(args: &Value) -> Result<Value, Value> {
    let query = args
        .get("query")
        .and_then(|q| q.as_str())
        .ok_or_else(|| json_rpc_error(-32602, "Missing required parameter: query"))?;
    let path_str = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
    let num_results = args
        .get("num_results")
        .and_then(|n| n.as_u64())
        .unwrap_or(10) as usize;

    let path = Path::new(path_str)
        .canonicalize()
        .map_err(|_| json_rpc_error(-32602, &format!("Path not found: {path_str}")))?;

    let (index_root, existing) = index::find_index_root(&path);
    if existing.is_none() {
        return Err(json_rpc_error(
            -32000,
            "No index found. Run 'og build' first.",
        ));
    }

    let model = resolve_index_model(&index_root);
    let mut idx = SemanticIndex::new_with_model(&index_root, None, model)
        .map_err(|e| json_rpc_error(-32000, &e.to_string()))?;

    // Auto-update stale files
    let files = walker::scan(&index_root).map_err(|e| json_rpc_error(-32000, &e.to_string()))?;
    let stale = idx
        .needs_update(&files)
        .map_err(|e| json_rpc_error(-32000, &e.to_string()))?;
    if stale > 0 {
        idx.update(&files, model.batch_size)
            .map_err(|e| json_rpc_error(-32000, &e.to_string()))?;
    }

    idx.set_search_scope(Some(&path));
    let mut results = idx
        .search(query, num_results)
        .map_err(|e| json_rpc_error(-32000, &e.to_string()))?;

    boost_results(&mut results, query);

    let text = results
        .iter()
        .map(|r| {
            let content = r.content.as_deref().unwrap_or("");
            format!(
                "## {}:{} ({}, score: {:.2})\n```\n{}\n```",
                r.file, r.line, r.name, r.score, content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

fn tool_similar(args: &Value) -> Result<Value, Value> {
    let reference = args
        .get("reference")
        .and_then(|r| r.as_str())
        .ok_or_else(|| json_rpc_error(-32602, "Missing required parameter: reference"))?;
    let num_results = args
        .get("num_results")
        .and_then(|n| n.as_u64())
        .unwrap_or(10) as usize;

    // Parse reference: file#name, file:line, or file
    let (file_path, line, name) = if let Some(hash_pos) = reference.rfind('#') {
        let file = &reference[..hash_pos];
        let n = &reference[hash_pos + 1..];
        (file, None, Some(n))
    } else if let Some(colon_pos) = reference.rfind(':') {
        let file = &reference[..colon_pos];
        let l = reference[colon_pos + 1..]
            .parse::<usize>()
            .map_err(|_| json_rpc_error(-32602, "Invalid line number"))?;
        (file, Some(l), None)
    } else {
        (reference, None, None)
    };

    let abs_path = Path::new(file_path)
        .canonicalize()
        .map_err(|_| json_rpc_error(-32602, &format!("File not found: {file_path}")))?;

    let file_dir = abs_path.parent().unwrap_or(Path::new("."));
    let (index_root, existing) = index::find_index_root(file_dir);
    if existing.is_none() {
        return Err(json_rpc_error(
            -32000,
            "No index found. Run 'og build' first.",
        ));
    }

    let model = resolve_index_model(&index_root);
    let idx = SemanticIndex::new_with_model(&index_root, None, model)
        .map_err(|e| json_rpc_error(-32000, &e.to_string()))?;

    let abs_str = abs_path.to_string_lossy();
    let results = idx
        .find_similar(&abs_str, line, name, num_results)
        .map_err(|e| json_rpc_error(-32000, &e.to_string()))?;

    let text = results
        .iter()
        .map(|r| {
            let content = r.content.as_deref().unwrap_or("");
            format!(
                "## {}:{} ({}, score: {:.2})\n```\n{}\n```",
                r.file, r.line, r.name, r.score, content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

fn tool_status(args: &Value) -> Result<Value, Value> {
    let path_str = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
    let path = Path::new(path_str)
        .canonicalize()
        .map_err(|_| json_rpc_error(-32602, &format!("Path not found: {path_str}")))?;

    let (index_root, existing) = index::find_index_root(&path);
    if existing.is_none() {
        return Ok(json!({
            "content": [{ "type": "text", "text": "No index found. Run 'og build' first." }]
        }));
    }

    let index_dir = index_root.join(INDEX_DIR);
    let manifest =
        Manifest::load(&index_dir).map_err(|e| json_rpc_error(-32000, &e.to_string()))?;

    let files = manifest.files.len();
    let blocks: usize = manifest.files.values().map(|e| e.blocks.len()).sum();

    let text = format!(
        "Index: {}\nModel: {}\nFiles: {files}\nBlocks: {blocks}",
        index_root.display(),
        manifest.model,
    );

    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

fn resolve_index_model(index_root: &Path) -> &'static embedder::ModelConfig {
    let index_dir = index_root.join(INDEX_DIR);
    match Manifest::load(&index_dir) {
        Ok(manifest) => embedder::resolve_model_by_version(&manifest.model),
        Err(_) => embedder::EDGE_MODEL,
    }
}
