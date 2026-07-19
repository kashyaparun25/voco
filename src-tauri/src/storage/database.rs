use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::Arc;
use crate::storage::migrations::run_migrations;
use parking_lot::{Mutex, MutexGuard};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL lets a second process (the read-only voco-mcp sidecar) read the DB
        // while the app writes segments mid-meeting — in the default rollback-journal
        // mode a reader's lock would make those inserts fail with SQLITE_BUSY.
        // busy_timeout gives either side a grace window on transient contention.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.busy_timeout(std::time::Duration::from_millis(3000));
        run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }


    // Settings helpers
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare_cached("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // Meetings helpers
    pub fn create_meeting(&self, id: &str, title: &str, source: &str) -> Result<()> {
        let conn = self.conn.lock();
        let created_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO meetings (id, title, created_at, duration, source) VALUES (?1, ?2, ?3, 0, ?4)",
            params![id, title, created_at, source],
        )?;
        Ok(())
    }

    pub fn update_meeting_duration(&self, id: &str, duration: i32) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE meetings SET duration = ?2 WHERE id = ?1",
            params![id, duration],
        )?;
        Ok(())
    }

    pub fn update_meeting_summary(&self, id: &str, summary: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE meetings SET summary = ?2 WHERE id = ?1",
            params![id, summary],
        )?;
        Ok(())
    }

    // Speakers helpers
    pub fn create_speaker(&self, id: &str, name: &str) -> Result<()> {
        let conn = self.conn.lock();
        let created_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO speakers (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, created_at],
        )?;
        Ok(())
    }

    // Segments helpers
    pub fn add_segment(
        &self,
        id: &str,
        meeting_id: &str,
        speaker_id: Option<&str>,
        start_time: f64,
        end_time: f64,
        text: &str,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let created_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO segments (id, meeting_id, speaker_id, start_time, end_time, text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, meeting_id, speaker_id, start_time, end_time, text, created_at],
        )?;
        Ok(())
    }

    /// Delete every transcript segment for a meeting (used before reprocessing
    /// a saved recording so the regenerated transcript replaces the old one).
    pub fn clear_segments(&self, meeting_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM segments WHERE meeting_id = ?1", params![meeting_id])?;
        Ok(())
    }

    /// Insert a speaker, or update its name if the id already exists.
    pub fn upsert_speaker(&self, id: &str, name: &str) -> Result<()> {
        let conn = self.conn.lock();
        let created_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO speakers (id, name, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![id, name, created_at],
        )?;
        Ok(())
    }

    /// Point a segment at a different speaker (used by the diarization relabel pass).
    pub fn update_segment_speaker(&self, segment_id: &str, speaker_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE segments SET speaker_id = ?2 WHERE id = ?1",
            params![segment_id, speaker_id],
        )?;
        Ok(())
    }

    // ── Dictation history ────────────────────────────────────────────────
    pub fn add_dictation(
        &self,
        id: &str,
        text: &str,
        duration_ms: i64,
        model: Option<&str>,
        audio_path: Option<&str>,
        app: Option<&str>,
        ai_enhanced: bool,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dictations (id, text, created_at, duration_ms, model, audio_path, app, ai_enhanced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, text, chrono::Utc::now().to_rfc3339(), duration_ms, model, audio_path, app, ai_enhanced as i64],
        )?;
        Ok(())
    }

    /// Delete all dictation history (used by "Reset All Stats").
    pub fn clear_dictations(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM dictations", [])?;
        Ok(())
    }

    /// Rows used to compute dictation stats: (created_at RFC3339, text,
    /// duration_ms, app, ai_enhanced).
    pub fn dictation_stat_rows(&self) -> Result<Vec<(String, String, i64, Option<String>, bool)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT created_at, text, duration_ms, app, ai_enhanced FROM dictations",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4).unwrap_or(0) != 0,
            ))
        })?;
        rows.collect()
    }

    /// Returns dictations newest-first: (id, text, created_at, duration_ms, model, audio_path).
    #[allow(clippy::type_complexity)]
    pub fn list_dictations(
        &self,
        limit: i64,
    ) -> Result<Vec<(String, String, String, i64, Option<String>, Option<String>)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, text, created_at, duration_ms, model, audio_path
             FROM dictations ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_dictation_audio_path(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT audio_path FROM dictations WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(row.get::<_, Option<String>>(0)?)
        } else {
            Ok(None)
        }
    }

    pub fn delete_dictation(&self, id: &str) -> Result<Option<String>> {
        let audio = self.get_dictation_audio_path(id)?;
        let conn = self.conn.lock();
        conn.execute("DELETE FROM dictations WHERE id = ?1", params![id])?;
        Ok(audio)
    }

    /// Returns audio paths of dictations beyond the newest `keep`, so callers can
    /// delete those files (transcript rows are kept; only audio is pruned).
    pub fn prune_dictation_audio(&self, keep: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT audio_path FROM dictations
             WHERE audio_path IS NOT NULL
             ORDER BY created_at DESC LIMIT -1 OFFSET ?1",
        )?;
        let rows = stmt.query_map(params![keep], |r| r.get::<_, Option<String>>(0))?;
        let mut paths = Vec::new();
        for r in rows {
            if let Some(p) = r? {
                paths.push(p);
            }
        }
        // Null out the pruned audio paths.
        conn.execute(
            "UPDATE dictations SET audio_path = NULL WHERE id IN (
                SELECT id FROM dictations WHERE audio_path IS NOT NULL
                ORDER BY created_at DESC LIMIT -1 OFFSET ?1)",
            params![keep],
        )?;
        Ok(paths)
    }

    /// Full-text-ish search across all transcript segments. Returns matches with
    /// meeting + speaker context, most recent meetings first.
    #[allow(clippy::type_complexity)]
    pub fn search_segments(
        &self,
        query: &str,
    ) -> Result<Vec<(String, String, String, String, f64, Option<String>)>> {
        let conn = self.conn.lock();
        let like = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT s.meeting_id, m.title, s.id, s.text, s.start_time, sp.name
             FROM segments s
             JOIN meetings m ON s.meeting_id = m.id
             LEFT JOIN speakers sp ON s.speaker_id = sp.id
             WHERE s.text LIKE ?1
             ORDER BY m.created_at DESC, s.start_time ASC
             LIMIT 200",
        )?;
        let rows = stmt.query_map(params![like], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Return `(segment_id, start_time, end_time)` for every segment in a meeting,
    /// ordered by start time.
    pub fn list_segment_spans(&self, meeting_id: &str) -> Result<Vec<(String, f64, f64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, start_time, end_time FROM segments WHERE meeting_id = ?1 ORDER BY start_time ASC",
        )?;
        let rows = stmt.query_map(params![meeting_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}
