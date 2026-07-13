use rusqlite::Connection;
use log::info;

pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    info!("Running database migrations...");

    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON;", [])?;

    // Create settings table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
        [],
    )?;

    // Create meetings table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            duration INTEGER NOT NULL DEFAULT 0,
            summary TEXT
        );",
        [],
    )?;

    // Create speakers table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS speakers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            profile_data BLOB,
            created_at TEXT NOT NULL
        );",
        [],
    )?;

    // Create segments table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS segments (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL,
            speaker_id TEXT,
            start_time REAL NOT NULL,
            end_time REAL NOT NULL,
            text TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
            FOREIGN KEY(speaker_id) REFERENCES speakers(id) ON DELETE SET NULL
        );",
        [],
    )?;

    // Create dictations table (history of quick dictations)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS dictations (
            id TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            created_at TEXT NOT NULL,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            model TEXT,
            audio_path TEXT
        );",
        [],
    )?;

    // Additive columns for stats (frontmost app + whether AI enhancement ran).
    // ALTER ... ADD COLUMN errors if the column already exists — ignore that.
    let _ = conn.execute("ALTER TABLE dictations ADD COLUMN app TEXT", []);
    let _ = conn.execute("ALTER TABLE dictations ADD COLUMN ai_enhanced INTEGER NOT NULL DEFAULT 0", []);

    // Distinguishes recorded meetings from imported audio files ("recording" | "import").
    let _ = conn.execute("ALTER TABLE meetings ADD COLUMN source TEXT NOT NULL DEFAULT 'recording'", []);

    info!("Database migrations completed successfully.");
    Ok(())
}
