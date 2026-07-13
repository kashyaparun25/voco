use tauri::State;
use crate::state::AppState;
use crate::services::export;
use log::info;

/// Exports a meeting transcript and returns the rendered content as a string.
/// The frontend can then use the dialog/fs plugins to save it.
#[tauri::command]
pub fn export_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
    format: String,
) -> Result<String, String> {
    export::export_meeting(&state.db, &meeting_id, &format)
}

/// Exports a meeting transcript directly to a file on disk. Returns the path written.
#[tauri::command]
pub fn export_meeting_to_path(
    state: State<'_, AppState>,
    meeting_id: String,
    format: String,
    path: String,
) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("Export path cannot be empty".to_string());
    }
    let content = export::export_meeting(&state.db, &meeting_id, &format)?;
    std::fs::write(&path, content).map_err(|e| format!("Failed to write export file: {}", e))?;
    info!("Exported meeting {} to {}", meeting_id, path);
    Ok(path)
}
