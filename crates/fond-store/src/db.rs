use std::path::Path;

use refinery::embed_migrations;
use rusqlite::Connection;

embed_migrations!("migrations");

/// A fond SQLite database connection.
///
/// Wraps a `rusqlite::Connection` with migrations, WAL mode,
/// and foreign key enforcement. The database is a derived index —
/// `.cook` files are the source of truth.
pub struct FondDb {
    conn: Connection,
}

impl FondDb {
    /// Open (or create) a database file and run migrations.
    pub fn open(path: &Path) -> Result<Self, crate::StoreError> {
        let mut conn = Connection::open(path).map_err(|e| crate::StoreError::Database {
            message: format!("failed to open database: {e}"),
        })?;
        Self::init(&mut conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database for tests.
    pub fn open_memory() -> Result<Self, crate::StoreError> {
        let mut conn = Connection::open_in_memory().map_err(|e| crate::StoreError::Database {
            message: format!("failed to open in-memory database: {e}"),
        })?;
        Self::init(&mut conn)?;
        Ok(Self { conn })
    }

    fn init(conn: &mut Connection) -> Result<(), crate::StoreError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| crate::StoreError::Database {
                message: format!("failed to enable foreign keys: {e}"),
            })?;

        let _: String = conn
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(|e| crate::StoreError::Database {
                message: format!("failed to set WAL mode: {e}"),
            })?;

        migrations::runner()
            .run(conn)
            .map_err(|e| crate::StoreError::Migration {
                message: format!("{e}"),
            })?;

        Ok(())
    }

    /// Get a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
