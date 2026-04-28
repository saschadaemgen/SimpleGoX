//! Schema migration runner for SimpleGoX's SQLite store.
//!
//! Migrations are applied in order at `QueueStore::open` time, gated by the
//! `sgx_migrations` table. Each migration is identified by a unique name
//! following `M{YYYYMMDD}_{NNN}_{slug}` convention (compatible with
//! simplexmq's naming style).
//!
//! New migrations: append to `all_migrations()` only; never reorder, never
//! edit existing entries. SQL files live under `migrations/` and are
//! inlined via `include_str!`.
//!
//! Briefing 044g.1a: introduced as part of the connections-table refactor.
//! This is the first time SimpleGoX has versioned schema migrations. The
//! defensive `ALTER TABLE ADD COLUMN ... .ok()` pattern in `queue_store.rs`
//! continues to work for column-additions on already-migrated DBs but no
//! new defensive ALTERs should be added once a migration runner exists -
//! prefer named migrations going forward.

use anyhow::{Context, Result};
use rusqlite::Connection;

pub struct Migration {
    pub name: &'static str,
    pub up_sql: &'static str,
}

pub fn all_migrations() -> Vec<Migration> {
    vec![
        Migration {
            name: "M20260427_001_connections_table",
            up_sql: include_str!("migrations/M20260427_001_connections_table.sql"),
        },
        // Briefing 044g.1a-fix1: normalise legacy free-form conn_status
        // values ('pending', 'pending_hello', 'connected') to the
        // canonical vocabulary set by 044g.1a's connections table.
        Migration {
            name: "M20260427_002_normalize_conn_status",
            up_sql: include_str!("migrations/M20260427_002_normalize_conn_status.sql"),
        },
        // Future migrations append here. Never reorder, never edit existing.
    ]
}

/// Ensures the `sgx_migrations` tracking table exists. Idempotent.
fn ensure_migrations_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sgx_migrations (
            name        TEXT PRIMARY KEY,
            applied_at  INTEGER NOT NULL
        )",
        [],
    )
    .context("ensure sgx_migrations table")?;
    Ok(())
}

/// Returns the set of migration names already applied.
fn applied_migrations(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT name FROM sgx_migrations")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Apply all pending migrations in order. Returns the number of migrations
/// that were freshly applied.
///
/// Each migration's SQL file owns its own `BEGIN; ... COMMIT;` so the
/// transaction boundary is in the SQL. On `execute_batch` failure we
/// attempt a defensive `ROLLBACK` in case the SQL left a transaction open
/// (SQLite does not auto-rollback on a single-statement error inside a
/// BEGIN block) and propagate the error so the caller aborts startup.
pub fn apply_pending(conn: &mut Connection) -> Result<usize> {
    ensure_migrations_table(conn)?;
    let applied = applied_migrations(conn)?;
    let migrations = all_migrations();
    let mut count = 0usize;

    for m in &migrations {
        if applied.contains(m.name) {
            tracing::debug!(migration = m.name, "already applied, skipping");
            continue;
        }
        tracing::info!(migration = m.name, "applying migration");

        // The SQL file owns its BEGIN/COMMIT. execute_batch runs the
        // whole file as a sequence of statements; on partial failure the
        // transaction may be left open, which we roll back below.
        if let Err(e) = conn.execute_batch(m.up_sql) {
            let _ = conn.execute("ROLLBACK", []);
            return Err(e).with_context(|| format!("apply migration {}", m.name));
        }

        // The migration's SQL must INSERT into sgx_migrations as its last
        // step (see SQL file convention). Verify it did - this catches
        // SQL files that forgot the registration line.
        let registered: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sgx_migrations WHERE name = ?1",
            rusqlite::params![m.name],
            |row| row.get(0),
        )?;
        if registered != 1 {
            anyhow::bail!(
                "migration {} did not register itself in sgx_migrations \
                 (SQL file is missing the registration INSERT)",
                m.name
            );
        }

        count += 1;
        tracing::info!(migration = m.name, "applied successfully");
    }

    Ok(count)
}
