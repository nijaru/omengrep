//! SQLite catalog: files, blocks, FTS5 (terms + trigram), manifest metadata.
//! Immutable per generation; built fresh, opened read-only for search.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::types::Block;

/// Catalog schema version. Bump on incompatible changes.
pub const SCHEMA_VERSION: i64 = 1;

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("opening catalog {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

/// Open a published generation's catalog read-only (immutable assumption).
pub fn open_readonly(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("opening catalog {}", path.display()))?;
    conn.pragma_update(None, "query_only", "ON")?;
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY,          -- relative to index root
            size INTEGER NOT NULL,
            mtime INTEGER NOT NULL,
            hash TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS blocks (
            id INTEGER PRIMARY KEY,         -- row id, stable within generation
            block_key TEXT NOT NULL UNIQUE, -- "rel/path:start_line:name"
            file TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
            block_type TEXT NOT NULL,
            name TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            content TEXT NOT NULL,
            skeleton TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_blocks_file ON blocks(file);

        -- FTS5 with identifier splitting: search terms channel.
        -- split_identifiers appends camelCase/snake_case splits so
        -- "get user profile" matches getUserProfile.
        -- block_id (UNINDEXED) carries the integer blocks.id join key.
        CREATE VIRTUAL TABLE IF NOT EXISTS block_fts USING fts5(
            block_id UNINDEXED,
            terms,
            tokenize = 'unicode61 tokenchars ''_'''
        );

        -- Trigram channel for substring identifiers (ripgrep-in-index).
        -- substring matching of "UserProfile" inside "getUserProfile".
        CREATE VIRTUAL TABLE IF NOT EXISTS block_trigram USING fts5(
            block_id UNINDEXED,
            trigram,
            tokenize = 'trigram'
        );

        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let row = conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    );
    match row {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Insert a file and its blocks in one transaction.
pub fn insert_file(
    conn: &Connection,
    rel_path: &str,
    size: i64,
    mtime: i64,
    hash: &str,
    blocks: &[Block],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    insert_file_inner(&tx, rel_path, size, mtime, hash, blocks)?;
    tx.commit()?;
    Ok(())
}

fn insert_file_inner(
    tx: &rusqlite::Transaction<'_>,
    rel_path: &str,
    size: i64,
    mtime: i64,
    hash: &str,
    blocks: &[Block],
) -> Result<()> {
    tx.execute(
        "INSERT INTO files(path, size, mtime, hash) VALUES (?1, ?2, ?3, ?4)",
        params![rel_path, size, mtime, hash],
    )?;

    for block in blocks {
        let searchable = crate::tokenize::split_identifiers(&block.embedding_text());
        tx.execute(
            "INSERT INTO blocks(block_key, file, block_type, name, start_line, end_line, content, skeleton)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                block.id,
                block.file,
                block.block_type,
                block.name,
                block.start_line as i64,
                block.end_line as i64,
                block.content,
                block.skeleton,
            ],
        )?;
        let block_id = tx.last_insert_rowid();

        tx.execute(
            "INSERT INTO block_fts(block_id, terms) VALUES (?1, ?2)",
            params![block_id, searchable],
        )?;
        tx.execute(
            "INSERT INTO block_trigram(block_id, trigram) VALUES (?1, ?2)",
            params![block_id, block.name],
        )?;
    }

    Ok(())
}
