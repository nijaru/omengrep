//! og-core: semantic code search on owned SQLite + mmap storage.
//!
//! Layout follows ai/design/rust-rewrite-2026-09.md:
//! - scan:      ignore-crate walker, gitignore semantics
//! - extract:   tree-sitter blocks + prose chunking (ported from og v0.0.3)
//! - catalog:   SQLite: files, blocks, FTS5 terms + trigram, manifest
//! - vectors:   mmap fp16 sidecar, rayon exact scan
//! - model:     Embedder trait; deterministic test embedder (potion in tk-7wp8)
//! - retrieve:  BM25 + trigram + vector channels, RRF fusion
//! - index:     generation build + atomic CURRENT publish + open
//! - search:    query surface incl. file#name / file:line refs
//! - boost/output/tokenize/synonyms: ported ranking + presentation policy

pub mod boost;
pub mod catalog;
pub mod extract;
pub mod index;
pub mod manifest;
pub mod model;
pub mod output;
pub mod retrieve;
pub mod scan;
pub mod search;
pub mod synonyms;
pub mod tokenize;
pub mod types;
pub mod vectors;

pub use types::{EXIT_ERROR, EXIT_MATCH, EXIT_NO_MATCH};
