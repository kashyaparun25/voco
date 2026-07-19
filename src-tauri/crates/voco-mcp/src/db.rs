//! Read-only access to Voco's SQLite database (`voco.db`).
//!
//! The app owns the schema and migrations; this module only ever reads. It
//! opens the DB with `SQLITE_OPEN_READ_ONLY` and `query_only`, so a bug here can
//! never corrupt a user's meetings. The settings-key formats below MIRROR the
//! app (`meeting_notes::{id}`, `ai_title::{id}`, `custom_dictionary`,
//! `mcp_enabled`) — keep them in sync if the app changes them.

use anyhow::{anyhow, Result};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OpenFlags};
use serde_json::{json, Value};
use std::path::Path;

pub const MCP_ENABLED_KEY: &str = "mcp_enabled";
pub const CUSTOM_DICTIONARY_KEY: &str = "custom_dictionary";

pub fn meeting_notes_key(id: &str) -> String {
    format!("meeting_notes::{id}")
}
pub fn ai_title_key(id: &str) -> String {
    format!("ai_title::{id}")
}

/// Tables the app is expected to have created. If any are missing the DB is from
/// an older/newer app or not initialized, and tools report a clear error.
const REQUIRED_TABLES: &[&str] = &["settings", "meetings", "segments", "speakers", "dictations"];

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open_readonly(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        // Wait rather than error if the app is mid-write; belt-and-braces read-only.
        conn.busy_timeout(std::time::Duration::from_millis(2000))?;
        let _ = conn.pragma_update(None, "query_only", "ON");
        Ok(Self { conn })
    }

    /// True if every expected table exists.
    pub fn schema_ok(&self) -> bool {
        REQUIRED_TABLES.iter().all(|t| {
            self.conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                    params![t],
                    |_| Ok(()),
                )
                .is_ok()
        })
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .ok()
    }

    pub fn mcp_enabled(&self) -> bool {
        matches!(
            self.get_setting(MCP_ENABLED_KEY).as_deref(),
            Some("true") | Some("1")
        )
    }

    pub fn count(&self, table: &str) -> i64 {
        // table name is from a fixed internal set, never user input.
        self.conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(0)
    }

    fn notes_for(&self, id: &str) -> Option<String> {
        self.get_setting(&meeting_notes_key(id))
            .filter(|s| !s.trim().is_empty())
    }

    fn ai_title_for(&self, id: &str) -> Option<String> {
        self.get_setting(&ai_title_key(id))
            .filter(|s| !s.trim().is_empty())
    }

    /// Distinct speaker display names in a meeting (nulls skipped), ordered.
    fn speaker_names(&self, meeting_id: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT DISTINCT sp.name FROM segments s
             JOIN speakers sp ON s.speaker_id = sp.id
             WHERE s.meeting_id = ?1 AND sp.name IS NOT NULL AND sp.name <> ''
             ORDER BY sp.name",
        ) {
            if let Ok(rows) = stmt.query_map(params![meeting_id], |r| r.get::<_, String>(0)) {
                for r in rows.flatten() {
                    out.push(r);
                }
            }
        }
        out
    }

    // ── Tools ────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub fn list_meetings(
        &self,
        limit: i64,
        source: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        query: Option<&str>,
    ) -> Result<Value> {
        let mut sql = String::from(
            "SELECT id, title, created_at, duration, summary, source FROM meetings WHERE 1=1",
        );
        let mut binds: Vec<SqlValue> = Vec::new();
        if let Some(src) = source {
            if src != "all" && !src.is_empty() {
                sql.push_str(" AND source = ?");
                binds.push(SqlValue::Text(src.to_string()));
            }
        }
        if let Some(f) = from {
            sql.push_str(" AND created_at >= ?");
            binds.push(SqlValue::Text(f.to_string()));
        }
        if let Some(t) = to {
            sql.push_str(" AND created_at <= ?");
            binds.push(SqlValue::Text(t.to_string()));
        }
        if let Some(q) = query {
            if !q.is_empty() {
                sql.push_str(" AND title LIKE ?");
                binds.push(SqlValue::Text(format!("%{q}%")));
            }
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        binds.push(SqlValue::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, title, created_at, duration, summary, source) = row?;
            let ai = self.ai_title_for(&id);
            out.push(json!({
                "id": id,
                "title": display_title(&title, ai.as_deref()),
                "stored_title": title,
                "ai_title": ai,
                "created_at": created_at,
                "duration_s": duration,
                "source": source,
                "has_summary": summary.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false),
                "has_notes": self.notes_for(&id).is_some(),
                "speakers": self.speaker_names(&id),
            }));
        }
        Ok(json!({ "meetings": out, "count": out.len() }))
    }

    pub fn get_meeting(&self, meeting_id: &str) -> Result<Value> {
        let id = self.resolve_meeting_id(meeting_id)?;
        let (title, created_at, duration, summary, source) = self
            .conn
            .query_row(
                "SELECT title, created_at, duration, summary, source FROM meetings WHERE id=?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .map_err(|_| anyhow!("No meeting with id '{meeting_id}'"))?;
        let segment_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM segments WHERE meeting_id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let ai = self.ai_title_for(&id);
        Ok(json!({
            "id": id,
            "title": display_title(&title, ai.as_deref()),
            "stored_title": title,
            "ai_title": ai,
            "created_at": created_at,
            "duration_s": duration,
            "source": source,
            "summary": summary,
            "notes": self.notes_for(&id),
            "speakers": self.speaker_names(&id),
            "segment_count": segment_count,
        }))
    }

    /// Returns (rendered_or_json, truncated, next_offset_s).
    pub fn get_transcript(
        &self,
        meeting_id: &str,
        format: &str,
        offset_s: f64,
        limit_chars: usize,
    ) -> Result<Value> {
        let id = self.resolve_meeting_id(meeting_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT s.start_time, s.end_time, s.text, sp.name
             FROM segments s LEFT JOIN speakers sp ON s.speaker_id = sp.id
             WHERE s.meeting_id = ?1 AND s.start_time >= ?2
             ORDER BY s.start_time ASC",
        )?;
        let rows = stmt.query_map(params![id, offset_s], |r| {
            Ok((
                r.get::<_, f64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;

        let mut segs: Vec<(f64, f64, String, String)> = Vec::new();
        for row in rows {
            let (start, end, text, name) = row?;
            let speaker = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Speaker".into());
            segs.push((start, end, text, speaker));
        }

        // Accumulate up to limit_chars; always emit at least one segment.
        let mut included: Vec<&(f64, f64, String, String)> = Vec::new();
        let mut truncated = false;
        let mut next_offset: Option<f64> = None;
        let mut running = 0usize;
        for seg in &segs {
            let piece_len = seg.2.len() + seg.3.len() + 16;
            if !included.is_empty() && running + piece_len > limit_chars {
                truncated = true;
                next_offset = Some(seg.0);
                break;
            }
            running += piece_len;
            included.push(seg);
        }

        let body = match format {
            "json" => {
                let arr: Vec<Value> = included
                    .iter()
                    .map(|(s, e, t, sp)| json!({"start_s": s, "end_s": e, "speaker": sp, "text": t}))
                    .collect();
                serde_json::to_string_pretty(&arr).unwrap_or_default()
            }
            "markdown" => included
                .iter()
                .map(|(s, _, t, sp)| format!("**{}** ({}): {}", sp, fmt_ts(*s), t))
                .collect::<Vec<_>>()
                .join("\n\n"),
            _ => included
                .iter()
                .map(|(s, _, t, sp)| format!("[{}] {}: {}", fmt_ts(*s), sp, t))
                .collect::<Vec<_>>()
                .join("\n"),
        };

        Ok(json!({
            "meeting_id": id,
            "format": format,
            "transcript": body,
            "segments_returned": included.len(),
            "segments_total_from_offset": segs.len(),
            "truncated": truncated,
            "next_offset_s": next_offset,
        }))
    }

    pub fn search(&self, query: &str, scope: &str, limit: i64) -> Result<Value> {
        let like = format!("%{query}%");
        let mut hits: Vec<Value> = Vec::new();

        if scope == "meetings" || scope == "all" {
            let mut stmt = self.conn.prepare(
                "SELECT s.meeting_id, m.title, s.id, s.text, s.start_time, sp.name
                 FROM segments s
                 JOIN meetings m ON s.meeting_id = m.id
                 LEFT JOIN speakers sp ON s.speaker_id = sp.id
                 WHERE s.text LIKE ?1
                 ORDER BY m.created_at DESC, s.start_time ASC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![like, limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, f64>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })?;
            for row in rows {
                let (mid, mtitle, sid, text, start, name) = row?;
                hits.push(json!({
                    "type": "meeting",
                    "meeting_id": mid,
                    "meeting_title": mtitle,
                    "segment_id": sid,
                    "start_s": start,
                    "timestamp": fmt_ts(start),
                    "speaker": name.unwrap_or_else(|| "Speaker".into()),
                    "text": text,
                }));
            }
        }

        if scope == "dictations" || scope == "all" {
            let mut stmt = self.conn.prepare(
                "SELECT id, text, created_at, app FROM dictations
                 WHERE text LIKE ?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![like, limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })?;
            for row in rows {
                let (id, text, created_at, app) = row?;
                hits.push(json!({
                    "type": "dictation",
                    "id": id,
                    "created_at": created_at,
                    "app": app,
                    "text": text,
                }));
            }
        }

        hits.truncate(limit as usize);
        Ok(json!({ "query": query, "scope": scope, "hits": hits, "count": hits.len() }))
    }

    pub fn list_dictations(&self, limit: i64, since: Option<&str>, app: Option<&str>) -> Result<Value> {
        let mut sql = String::from(
            "SELECT id, text, created_at, duration_ms, model, app FROM dictations WHERE 1=1",
        );
        let mut binds: Vec<SqlValue> = Vec::new();
        if let Some(s) = since {
            sql.push_str(" AND created_at >= ?");
            binds.push(SqlValue::Text(s.to_string()));
        }
        if let Some(a) = app {
            if !a.is_empty() {
                sql.push_str(" AND app LIKE ?");
                binds.push(SqlValue::Text(format!("%{a}%")));
            }
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        binds.push(SqlValue::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds), |r| {
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
        for row in rows {
            let (id, text, created_at, duration_ms, model, app) = row?;
            out.push(json!({
                "id": id,
                "text": truncate(&text, 500),
                "text_truncated": text.chars().count() > 500,
                "created_at": created_at,
                "duration_ms": duration_ms,
                "model": model,
                "app": app,
            }));
        }
        Ok(json!({ "dictations": out, "count": out.len() }))
    }

    pub fn get_dictation(&self, id: &str) -> Result<Value> {
        let sql = if id == "latest" {
            "SELECT id, text, created_at, duration_ms, model, app, ai_enhanced
             FROM dictations ORDER BY created_at DESC LIMIT 1"
                .to_string()
        } else {
            "SELECT id, text, created_at, duration_ms, model, app, ai_enhanced
             FROM dictations WHERE id=?1"
                .to_string()
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "text": r.get::<_, String>(1)?,
                "created_at": r.get::<_, String>(2)?,
                "duration_ms": r.get::<_, i64>(3)?,
                "model": r.get::<_, Option<String>>(4)?,
                "app": r.get::<_, Option<String>>(5)?,
                "ai_enhanced": r.get::<_, i64>(6).unwrap_or(0) != 0,
            }))
        };
        let row = if id == "latest" {
            stmt.query_row([], map)
        } else {
            stmt.query_row(params![id], map)
        };
        row.map_err(|_| anyhow!("No dictation found for '{id}'"))
    }

    pub fn get_dictionary(&self) -> Result<Value> {
        match self.get_setting(CUSTOM_DICTIONARY_KEY) {
            Some(raw) => {
                let parsed: Value = serde_json::from_str(&raw)
                    .unwrap_or_else(|_| Value::String(raw.clone()));
                let count = parsed.as_array().map(|a| a.len()).unwrap_or(0);
                Ok(json!({ "entries": parsed, "count": count }))
            }
            None => Ok(json!({ "entries": [], "count": 0 })),
        }
    }

    /// Accept a real id or the literal "latest" (most recent meeting).
    fn resolve_meeting_id(&self, id: &str) -> Result<String> {
        if id == "latest" {
            self.conn
                .query_row(
                    "SELECT id FROM meetings ORDER BY created_at DESC LIMIT 1",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map_err(|_| anyhow!("No meetings yet"))
        } else {
            Ok(id.to_string())
        }
    }
}

/// Prefer the AI-suggested title only when the stored one is empty or a default.
fn display_title(stored: &str, ai: Option<&str>) -> String {
    let s = stored.trim();
    let looks_default = s.is_empty()
        || s.eq_ignore_ascii_case("new meeting")
        || s.eq_ignore_ascii_case("untitled")
        || s.eq_ignore_ascii_case("meeting")
        || s.eq_ignore_ascii_case("imported audio");
    match (looks_default, ai) {
        (true, Some(a)) if !a.trim().is_empty() => a.trim().to_string(),
        _ => stored.to_string(),
    }
}

fn fmt_ts(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_title_prefers_ai_only_for_defaults() {
        assert_eq!(display_title("Q3 Planning", Some("AI title")), "Q3 Planning");
        assert_eq!(display_title("New Meeting", Some("AI title")), "AI title");
        assert_eq!(display_title("  ", Some("AI title")), "AI title");
        assert_eq!(display_title("New Meeting", None), "New Meeting");
        assert_eq!(display_title("New Meeting", Some("   ")), "New Meeting");
    }

    #[test]
    fn fmt_ts_switches_to_hours() {
        assert_eq!(fmt_ts(0.0), "00:00");
        assert_eq!(fmt_ts(65.4), "01:05");
        assert_eq!(fmt_ts(3661.0), "01:01:01");
        assert_eq!(fmt_ts(-5.0), "00:00");
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 3), "hel…");
        // Multi-byte chars must not panic on a byte boundary.
        assert_eq!(truncate("héllo→", 2), "hé…");
    }
}
