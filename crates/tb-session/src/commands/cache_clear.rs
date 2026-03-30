use crate::config::Config;
use crate::error::Result;

pub fn run() -> Result<()> {
    let config = Config::load()?;
    let db_path = config.db_path()?;

    if db_path.exists() {
        std::fs::remove_file(&db_path)?;

        // Also remove SQLite WAL and shared-memory sidecar files if present.
        let wal = db_path.with_extension("db-wal");
        if wal.exists() {
            std::fs::remove_file(&wal)?;
        }
        let shm = db_path.with_extension("db-shm");
        if shm.exists() {
            std::fs::remove_file(&shm)?;
        }

        println!("Index cleared: {}", db_path.display());
    } else {
        println!("No index to clear.");
    }

    Ok(())
}
