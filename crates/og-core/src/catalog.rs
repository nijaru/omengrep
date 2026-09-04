//! SQLite catalog: files, blocks, FTS5 (terms + trigram), manifest metadata.
//! Immutable per generation; built fresh, opened read-only for search.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::types::Block;

/// Catalog + sidecar storage schema version. Bump on incompatible changes.
/// v2: vector sidecar rows are fp16 (was f32) — halves sidecar and scan RSS.
pub const SCHEMA_VERSION: i64 = 2;

pub fn open(path: &Path) -> Result<Connection> {
    let conn =
        Connection::open(path).with_context(|| format!("opening catalog {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

/// Open a published generation's catalog read-only (immutable assumption).
pub fn open_readonly(path: &Path) -> Result<Connection> {
    let conn =
        Connection::open(path).with_context(|| format!("opening catalog {}", path.display()))?;
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

/// Path -> (size, mtime) for every indexed file (incremental diff input).
pub fn file_fingerprints(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, (i64, i64)>> {
    let mut stmt = conn.prepare("SELECT path, size, mtime FROM files")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(|(p, s, m)| (p, (s, m))).collect())
}

/// Remove a file and all its blocks. Returns the deleted block rowids so the
/// caller can zero their vector rows. FTS rows are keyed by the same rowids.
pub fn delete_file(conn: &Connection, rel_path: &str) -> Result<Vec<i64>> {
    let tx = conn.unchecked_transaction()?;
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT id FROM blocks WHERE file = ?1")?;
        stmt.query_map(params![rel_path], |row| row.get::<_, i64>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    for id in &ids {
        tx.execute("DELETE FROM block_fts WHERE rowid = ?1", params![id])?;
        tx.execute("DELETE FROM block_trigram WHERE rowid = ?1", params![id])?;
    }
    // Files row delete cascades to blocks (FK ON DELETE CASCADE).
    tx.execute("DELETE FROM files WHERE path = ?1", params![rel_path])?;
    tx.commit()?;
    Ok(ids)
}

pub fn count_blocks(conn: &Connection) -> Result<usize> {
    Ok(conn.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get::<_, i64>(0))? as usize)
}

/// Files present in the catalog with zero indexed blocks (extraction
/// yielded nothing: below chunk minimum, no constructs).
pub fn count_empty_files(conn: &Connection) -> Result<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files f WHERE NOT EXISTS (SELECT 1 FROM blocks b WHERE b.file = f.path)",
        [],
        |r| r.get(0),
    )?;
    Ok(n as usize)
}

pub fn count_files(conn: &Connection) -> Result<usize> {
    Ok(conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))? as usize)
}

/// Insert a file and its blocks in one transaction. Returns the rowids
/// SQLite assigned, in block order (vector rows must be written at these
/// exact positions — SQLite may reuse a freed max rowid).
pub fn insert_file(
    conn: &Connection,
    rel_path: &str,
    size: i64,
    mtime: i64,
    hash: &str,
    blocks: &[Block],
) -> Result<Vec<i64>> {
    let tx = conn.unchecked_transaction()?;
    let rowids = insert_file_inner(&tx, rel_path, size, mtime, hash, blocks)?;
    tx.commit()?;
    Ok(rowids)
}

fn insert_file_inner(
    tx: &rusqlite::Transaction<'_>,
    rel_path: &str,
    size: i64,
    mtime: i64,
    hash: &str,
    blocks: &[Block],
) -> Result<Vec<i64>> {
    tx.execute(
        "INSERT INTO files(path, size, mtime, hash) VALUES (?1, ?2, ?3, ?4)",
        params![rel_path, size, mtime, hash],
    )?;

    let mut rowids = Vec::with_capacity(blocks.len());
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
        rowids.push(block_id);

        // FTS rowid = block rowid: deletes are O(log n) keyed lookups.
        tx.execute(
            "INSERT INTO block_fts(rowid, block_id, terms) VALUES (?1, ?1, ?2)",
            params![block_id, searchable],
        )?;
        tx.execute(
            "INSERT INTO block_trigram(rowid, block_id, trigram) VALUES (?1, ?1, ?2)",
            params![block_id, block.name],
        )?;
    }

    Ok(rowids)
}
