use tauri::State;
use crate::state::AppState;
use crate::audio::list_input_devices;

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<String>, String> {
    list_input_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_audio_device(state: State<'_, AppState>, device: String) -> Result<(), String> {
    state.db.set_setting("active_audio_device", &device).map_err(|e| e.to_string())
}
