//! The persistence seam: `Storage` trait + the SQLite reference impl.
//!
//! Every module's `storage.rs` talks through this seam (single writer, WAL,
//! parameterized queries only — §5.16 of AGENTS.md). `Database` is the SQLite
//! implementation and the reference backend; an in-memory variant exists so
//! tests run headlessly without touching the filesystem (§5.11).

use crate::errors::{AppError, Result};
use rusqlite::{Connection, Row, ToSql};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// The persistence boundary. Implementations are required to be `Send + Sync`
/// so they can be shared behind `Arc` across the async runtime.
///
/// All SQL goes through these methods with **bound parameters** — the app
/// never builds SQL by string interpolation (§5.16).
pub trait Storage: Send + Sync {
    /// Run a statement that does not return rows (INSERT/UPDATE/DELETE/DDL).
    fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> Result<usize>;

    /// Run a query returning at most one row; `None` when no row matched.
    fn query_row<T, F>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        map: F,
    ) -> Result<Option<T>>
    where
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>;

    /// Run a query returning many rows, mapped through `map`.
    fn query_vec<T, F>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        map: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(&Row<'_>) -> rusqlite::Result<T>;

    /// Run `work` inside one short transaction (single-writer discipline).
    ///
    /// The closure receives the connection with the transaction open; commit on
    /// success, rollback on error. Long-lived transactions spanning provider
    /// calls are forbidden (§5.16) — this is intentionally scoped.
    fn with_transaction<T, F>(&self, work: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>;
}

/// The SQLite reference implementation of [`Storage`].
///
/// WAL mode + a single connection behind a mutex gives us one writer, no
/// `database is locked` contention, and crash-safe appends. Reading is also
/// serialized here — acceptable for a single-user desktop app, and it keeps
/// the writer discipline trivially enforceable.
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the SQLite file at `path`, apply migrations, WAL on.
    pub fn open(path: &Path) -> Result<Database> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    /// Open an in-memory SQLite database — used by tests and headless runs.
    pub fn open_in_memory() -> Result<Database> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection)
    }

    fn from_connection(mut connection: Connection) -> Result<Database> {
        // WAL keeps readers from blocking the writer and vice versa.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        // Busy timeout so a briefly-locked DB waits instead of erroring.
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        // Foreign keys enforce ON DELETE CASCADE across campaigns.
        connection.pragma_update(None, "foreign_keys", "ON")?;

        // Schema changes go through migrations only (never in-place edits).
        crate::migrations::migrations().to_latest(&mut connection).map_err(
            |error| AppError::Storage(format!("migration failed: {error}")),
        )?;

        Ok(Database { connection: Mutex::new(connection) })
    }

    /// Lock the connection for a multi-statement block (tests, custom flows).
    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        // A poisoned mutex means a previous holder panicked while holding the
        // lock — fail closed with a typed error rather than unwrapping.
        self.connection.lock().map_err(|_poisoned| {
            AppError::Storage("connection lock poisoned".to_string())
        })
    }
}

impl Storage for Database {
    fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> Result<usize> {
        let guard = self.lock()?;
        guard.execute(sql, params).map_err(AppError::from)
    }

    fn query_row<T, F>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        map: F,
    ) -> Result<Option<T>>
    where
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        let guard = self.lock()?;
        let mut statement = guard.prepare(sql)?;
        match statement.query_row(params, map) {
            Ok(value) => Ok(Some(value)),
            // "No rows" is not an error — it is a legitimate `None`.
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(AppError::from(error)),
        }
    }

    fn query_vec<T, F>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        mut map: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
    {
        let guard = self.lock()?;
        let mut statement = guard.prepare(sql)?;
        let rows = statement.query_map(params, &mut map)?;
        // Collect eagerly so the borrowed statement is dropped before the lock.
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    fn with_transaction<T, F>(&self, work: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let mut guard = self.lock()?;
        let transaction = guard.transaction()?;
        match work(&transaction) {
            Ok(value) => {
                transaction.commit()?;
                Ok(value)
            }
            Err(error) => {
                // Rollback failure is secondary; the original error is the truth.
                let _ = transaction.rollback();
                Err(error)
            }
        }
    }
}
