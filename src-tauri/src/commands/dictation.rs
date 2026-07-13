use tauri::State;
use crate::state::AppState;
use serde::Serialize;

#[tauri::command]
pub fn start_dictation(state: State<'_, AppState>, app_handle: tauri::AppHandle) -> Result<(), String> {
    state.dictation_service.start(app_handle)
}

#[tauri::command]
pub fn stop_dictation(state: State<'_, AppState>) -> Result<(), String> {
    state.dictation_service.stop()
}

#[derive(Serialize)]
pub struct DictationEntry {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub duration_ms: i64,
    pub model: Option<String>,
    pub has_audio: bool,
}

/// Past dictations, newest first (default 200).
#[tauri::command]
pub fn get_dictations(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<DictationEntry>, String> {
    let rows = state.db.list_dictations(limit.unwrap_or(200)).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, text, created_at, duration_ms, model, audio_path)| DictationEntry {
            id,
            text,
            created_at,
            duration_ms,
            model,
            has_audio: audio_path.as_deref().map(|p| std::path::Path::new(p).exists()).unwrap_or(false),
        })
        .collect())
}

/// Absolute path to a dictation's saved audio (for playback), if present.
#[tauri::command]
pub fn get_dictation_audio_path(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let p = state.db.get_dictation_audio_path(&id).map_err(|e| e.to_string())?;
    Ok(p.filter(|p| !p.is_empty() && std::path::Path::new(p).exists()))
}

#[tauri::command]
pub fn delete_dictation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let audio = state.db.delete_dictation(&id).map_err(|e| e.to_string())?;
    if let Some(p) = audio {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}
