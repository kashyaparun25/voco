use crate::commands::meeting::Segment;
use crate::storage::Database;

/// A meeting's metadata used when producing exports.
pub struct MeetingMeta {
    pub title: String,
    pub created_at: String,
    pub summary: Option<String>,
}

/// Loads meeting metadata (title, created_at, summary) from the database.
pub fn get_meeting_meta(db: &Database, meeting_id: &str) -> Result<MeetingMeta, String> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare("SELECT title, created_at, summary FROM meetings WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let mut rows = stmt
        .query(rusqlite::params![meeting_id])
        .map_err(|e| e.to_string())?;

    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        Ok(MeetingMeta {
            title: row.get(0).map_err(|e| e.to_string())?,
            created_at: row.get(1).map_err(|e| e.to_string())?,
            summary: row.get(2).map_err(|e| e.to_string())?,
        })
    } else {
        Err(format!("Meeting {} not found", meeting_id))
    }
}

/// Loads all transcript segments (joined with speaker names) for a meeting, ordered by start time.
pub fn get_segments(db: &Database, meeting_id: &str) -> Result<Vec<Segment>, String> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.meeting_id, s.speaker_id, sp.name, s.start_time, s.end_time, s.text, s.created_at
             FROM segments s
             LEFT JOIN speakers sp ON s.speaker_id = sp.id
             WHERE s.meeting_id = ?1
             ORDER BY s.start_time ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([meeting_id], |row| {
            Ok(Segment {
                id: row.get(0)?,
                meeting_id: row.get(1)?,
                speaker_id: row.get(2)?,
                speaker_name: row.get(3)?,
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                text: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut segments = Vec::new();
    for row in rows {
        segments.push(row.map_err(|e| e.to_string())?);
    }
    Ok(segments)
}

fn speaker_label(seg: &Segment) -> String {
    seg.speaker_name
        .clone()
        .or_else(|| seg.speaker_id.clone())
        .unwrap_or_else(|| "Unknown Speaker".to_string())
}

/// Formats seconds into an SRT timestamp `HH:MM:SS,mmm`.
fn format_srt_timestamp(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Plain text export: one line per segment, prefixed with the speaker label.
pub fn to_txt(meta: &MeetingMeta, segments: &[Segment]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", meta.title));
    out.push_str(&format!("{}\n\n", meta.created_at));
    for seg in segments {
        out.push_str(&format!("{}: {}\n", speaker_label(seg), seg.text));
    }
    out
}

/// SRT subtitle export.
pub fn to_srt(segments: &[Segment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&format!("{}\n", i + 1));
        out.push_str(&format!(
            "{} --> {}\n",
            format_srt_timestamp(seg.start_time),
            format_srt_timestamp(seg.end_time)
        ));
        out.push_str(&format!("{}: {}\n\n", speaker_label(seg), seg.text));
    }
    out
}

/// JSON export containing metadata + structured segments.
pub fn to_json(meta: &MeetingMeta, segments: &[Segment]) -> Result<String, String> {
    let segs: Vec<serde_json::Value> = segments
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "speaker_id": s.speaker_id,
                "speaker_name": s.speaker_name,
                "start_time": s.start_time,
                "end_time": s.end_time,
                "text": s.text,
            })
        })
        .collect();

    let value = serde_json::json!({
        "title": meta.title,
        "created_at": meta.created_at,
        "summary": meta.summary,
        "segments": segs,
    });

    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

/// Markdown export with speaker labels and (optionally) the summary.
pub fn to_markdown(meta: &MeetingMeta, segments: &[Segment]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", meta.title));
    out.push_str(&format!("_{}_\n\n", meta.created_at));

    if let Some(summary) = &meta.summary {
        if !summary.trim().is_empty() {
            out.push_str("## Summary\n\n");
            out.push_str(summary.trim());
            out.push_str("\n\n");
        }
    }

    out.push_str("## Transcript\n\n");
    for seg in segments {
        out.push_str(&format!("**{}:** {}\n\n", speaker_label(seg), seg.text));
    }
    out
}

/// Renders a meeting to the requested format string.
/// Supported formats: "txt", "srt", "json", "md"/"markdown".
pub fn export_meeting(db: &Database, meeting_id: &str, format: &str) -> Result<String, String> {
    if meeting_id.trim().is_empty() {
        return Err("Meeting id cannot be empty".to_string());
    }

    let meta = get_meeting_meta(db, meeting_id)?;
    let segments = get_segments(db, meeting_id)?;

    match format.to_lowercase().as_str() {
        "txt" | "text" => Ok(to_txt(&meta, &segments)),
        "srt" => Ok(to_srt(&segments)),
        "json" => to_json(&meta, &segments),
        "md" | "markdown" => Ok(to_markdown(&meta, &segments)),
        other => Err(format!("Unsupported export format: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(speaker: Option<&str>, start: f64, end: f64, text: &str) -> Segment {
        Segment {
            id: "seg-1".into(),
            meeting_id: "m-1".into(),
            speaker_id: Some("spk_1".into()),
            speaker_name: speaker.map(|s| s.to_string()),
            start_time: start,
            end_time: end,
            text: text.into(),
            created_at: "2026-07-11T10:00:00Z".into(),
        }
    }

    fn meta() -> MeetingMeta {
        MeetingMeta {
            title: "Standup".into(),
            created_at: "2026-07-11T10:00:00Z".into(),
            summary: Some("A short summary.".into()),
        }
    }

    #[test]
    fn srt_timestamp_formats_hms_millis() {
        assert_eq!(format_srt_timestamp(0.0), "00:00:00,000");
        assert_eq!(format_srt_timestamp(1.5), "00:00:01,500");
        assert_eq!(format_srt_timestamp(65.25), "00:01:05,250");
        assert_eq!(format_srt_timestamp(3661.0), "01:01:01,000");
        // Negative clamps to zero.
        assert_eq!(format_srt_timestamp(-5.0), "00:00:00,000");
    }

    #[test]
    fn srt_has_sequential_indexes_and_arrow_timestamps() {
        let segs = vec![
            seg(Some("Alex"), 0.0, 2.0, "Hello"),
            seg(Some("Jordan"), 2.0, 4.5, "Hi there"),
        ];
        let out = to_srt(&segs);
        assert!(out.starts_with("1\n"));
        assert!(out.contains("00:00:00,000 --> 00:00:02,000"));
        assert!(out.contains("2\n00:00:02,000 --> 00:00:04,500"));
        assert!(out.contains("Alex: Hello"));
        assert!(out.contains("Jordan: Hi there"));
    }

    #[test]
    fn txt_includes_title_and_speaker_lines() {
        let out = to_txt(&meta(), &[seg(Some("Alex"), 0.0, 1.0, "One")]);
        assert!(out.starts_with("Standup\n"));
        assert!(out.contains("Alex: One"));
    }

    #[test]
    fn markdown_includes_summary_and_transcript() {
        let out = to_markdown(&meta(), &[seg(Some("Alex"), 0.0, 1.0, "One")]);
        assert!(out.contains("# Standup"));
        assert!(out.contains("## Summary"));
        assert!(out.contains("A short summary."));
        assert!(out.contains("## Transcript"));
        assert!(out.contains("**Alex:** One"));
    }

    #[test]
    fn missing_speaker_falls_back_to_label() {
        // No speaker_name, but speaker_id present -> uses id.
        let out = to_txt(&meta(), &[seg(None, 0.0, 1.0, "Anon")]);
        assert!(out.contains("spk_1: Anon"));
    }

    #[test]
    fn json_is_valid_and_contains_fields() {
        let out = to_json(&meta(), &[seg(Some("Alex"), 0.0, 1.5, "Hi")]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["title"], "Standup");
        assert_eq!(parsed["segments"][0]["speaker_name"], "Alex");
        assert_eq!(parsed["segments"][0]["text"], "Hi");
        assert_eq!(parsed["segments"][0]["end_time"], 1.5);
    }
}
