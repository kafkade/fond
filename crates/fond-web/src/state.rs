use std::path::PathBuf;
use std::sync::Mutex;

use fond_store::FondDb;

/// Shared application state for the web server.
///
/// `FondDb` wraps a `rusqlite::Connection` which is `!Send`,
/// so we use a `Mutex` for synchronized access. This is fine for
/// a single-household server with low concurrency.
#[derive(Clone)]
pub struct AppState {
    inner: std::sync::Arc<Inner>,
}

struct Inner {
    db: Mutex<FondDb>,
    data_dir: PathBuf,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> anyhow::Result<Self> {
        let db_path = data_dir.join("fond.db");
        let db =
            FondDb::open(&db_path).map_err(|e| anyhow::anyhow!("failed to open database: {e}"))?;

        Ok(Self {
            inner: std::sync::Arc::new(Inner {
                db: Mutex::new(db),
                data_dir,
            }),
        })
    }

    /// Run a closure with access to the database.
    ///
    /// Acquires the mutex, runs `f`, and returns the result.
    pub fn with_db<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&FondDb) -> T,
    {
        let db = self.inner.db.lock().expect("db mutex poisoned");
        f(&db)
    }

    /// Get the data directory path.
    pub fn data_dir(&self) -> &PathBuf {
        &self.inner.data_dir
    }

    /// Get the recipes directory path.
    pub fn recipes_dir(&self) -> PathBuf {
        self.inner.data_dir.join("recipes")
    }
}
