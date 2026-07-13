use tauri::State;
use crate::state::AppState;
use serde::Serialize;
use log::info;

#[derive(Serialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration: i32,
    pub summary: Option<String>,
    pub source: String,
}

#[derive(Serialize)]
pub struct Segment {
    pub id: String,
    pub meeting_id: String,
    pub speaker_id: Option<String>,
    pub speaker_name: Option<String>,
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
    pub created_at: String,
}

#[tauri::command]
pub fn start_meeting(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    title: String,
) -> Result<String, String> {
    state.meeting_service.start(app_handle, title)
}

#[tauri::command]
pub fn import_audio(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    path: String,
    title: String,
) -> Result<String, String> {
    state.meeting_service.import_audio_file(app_handle, path, title)
}

#[tauri::command]
pub fn stop_meeting(state: State<'_, AppState>) -> Result<(), String> {
    state.meeting_service.stop()
}

#[tauri::command]
pub fn reprocess_meeting(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    meeting_id: String,
) -> Result<(), String> {
    state.meeting_service.reprocess_meeting(app_handle, meeting_id)
}

#[tauri::command]
pub fn pause_meeting(state: State<'_, AppState>) -> Result<(), String> {
    state.meeting_service.pause()
}

#[tauri::command]
pub fn resume_meeting(state: State<'_, AppState>, app_handle: tauri::AppHandle) -> Result<(), String> {
    state.meeting_service.resume(app_handle)
}

#[tauri::command]
pub fn get_meetings(state: State<'_, AppState>) -> Result<Vec<Meeting>, String> {
    let conn = state.db.conn();
    let mut stmt = conn
        .prepare("SELECT id, title, created_at, duration, summary, source FROM meetings ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Meeting {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                duration: row.get(3)?,
                summary: row.get(4)?,
                source: row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "recording".to_string()),
            })
        })
        .map_err(|e| e.to_string())?;

    let mut meetings = Vec::new();
    for row in rows {
        meetings.push(row.map_err(|e| e.to_string())?);
    }
    Ok(meetings)
}

#[tauri::command]
pub fn delete_meeting(state: State<'_, AppState>, meeting_id: String) -> Result<(), String> {
    let conn = state.db.conn();
    conn.execute("DELETE FROM meetings WHERE id = ?1", rusqlite::params![meeting_id])
        .map_err(|e| e.to_string())?;
    info!("Deleted meeting: {}", meeting_id);
    Ok(())
}

#[tauri::command]
pub fn get_meeting_transcript(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<Segment>, String> {
    let conn = state.db.conn();
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

#[tauri::command]
pub fn rename_speaker(state: State<'_, AppState>, speaker_id: String, name: String) -> Result<(), String> {
    let conn = state.db.conn();
    conn.execute("INSERT OR REPLACE INTO speakers (id, name, created_at) VALUES (?1, ?2, ?3)", 
                 rusqlite::params![speaker_id, name, chrono::Utc::now().to_rfc3339()])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn add_meeting_segment(
    state: State<'_, AppState>,
    meeting_id: String,
    speaker_id: Option<String>,
    start_time: f64,
    end_time: f64,
    text: String,
) -> Result<String, String> {
    let segment_id = uuid::Uuid::new_v4().to_string();
    state.db.add_segment(
        &segment_id,
        &meeting_id,
        speaker_id.as_deref(),
        start_time,
        end_time,
        &text
    ).map_err(|e| e.to_string())?;
    Ok(segment_id)
}

#[tauri::command]
pub fn update_meeting_duration(state: State<'_, AppState>, meeting_id: String, duration: i32) -> Result<(), String> {
    state.db.update_meeting_duration(&meeting_id, duration)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
pub struct SearchHit {
    pub meeting_id: String,
    pub meeting_title: String,
    pub segment_id: String,
    pub text: String,
    pub start_time: f64,
    pub speaker_name: Option<String>,
}

/// Full-text search across every meeting's transcript.
#[tauri::command]
pub fn search_transcripts(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let rows = state.db.search_segments(q).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(meeting_id, meeting_title, segment_id, text, start_time, speaker_name)| SearchHit {
                meeting_id,
                meeting_title,
                segment_id,
                text,
                start_time,
                speaker_name,
            },
        )
        .collect())
}

/// Absolute path to a meeting's recorded audio, if it was saved and still exists.
#[tauri::command]
pub fn get_meeting_audio_path(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<String>, String> {
    let key = format!("audio_path::{}", meeting_id);
    let path = state.db.get_setting(&key).map_err(|e| e.to_string())?;
    Ok(path.filter(|p| !p.is_empty() && std::path::Path::new(p).exists()))
}
