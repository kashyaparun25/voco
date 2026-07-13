use serde_json::json;
use log::warn;
use tauri::State;
use crate::state::AppState;

fn recordings_path(state: &State<'_, AppState>) -> std::path::PathBuf {
    state
        .model_manager
        .models_dir()
        .parent()
        .map(|p| p.join("recordings"))
        .unwrap_or_else(|| std::path::PathBuf::from("recordings"))
}

/// Absolute path where meeting/dictation recordings are stored.
#[tauri::command]
pub fn get_recordings_dir(state: State<'_, AppState>) -> Result<String, String> {
    Ok(recordings_path(&state).to_string_lossy().to_string())
}

/// Reveal the recordings folder in Finder.
#[tauri::command]
pub fn reveal_recordings_dir(state: State<'_, AppState>) -> Result<(), String> {
    let dir = recordings_path(&state);
    let _ = std::fs::create_dir_all(&dir);
    std::process::Command::new("open")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("Failed to open folder: {}", e))?;
    Ok(())
}

/// Reads the total physical RAM of the machine in megabytes via `sysctl -n hw.memsize`.
#[tauri::command]
pub fn get_system_ram_mb() -> Result<u64, String> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .map_err(|e| format!("Failed to run sysctl: {}", e))?;

    if !output.status.success() {
        return Err("sysctl hw.memsize returned a non-zero exit status".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let bytes: u64 = stdout
        .trim()
        .parse()
        .map_err(|e| format!("Failed to parse hw.memsize: {}", e))?;

    Ok(bytes / (1024 * 1024))
}

/// Returns recommended STT + LLM model tiers based on the machine's RAM.
/// Tiers follow the implementation plan (sections 6.2 / 7.1):
///   < 16 GB  -> nano / tiny
///   16-31 GB -> small / medium
///   >= 32 GB -> large
#[tauri::command]
pub fn recommend_models() -> Result<serde_json::Value, String> {
    let ram_mb = match get_system_ram_mb() {
        Ok(v) => v,
        Err(e) => {
            // Graceful degradation: assume a conservative 8GB tier if detection fails.
            warn!("recommend_models: RAM detection failed ({}), assuming 8GB tier", e);
            8 * 1024
        }
    };

    let ram_gb = ram_mb as f64 / 1024.0;

    let (tier, stt_model, llm_model) = if ram_gb >= 32.0 {
        (
            "large",
            "whisper-large-v3-turbo",
            "qwen-1.5b-instruct-q4",
        )
    } else if ram_gb >= 16.0 {
        (
            "medium",
            "whisper-small-q5",
            "qwen-1.5b-instruct-q4",
        )
    } else {
        (
            "small",
            "whisper-tiny-q5",
            "qwen-1.5b-instruct-q4",
        )
    };

    Ok(json!({
        "ram_mb": ram_mb,
        "ram_gb": (ram_gb * 10.0).round() / 10.0,
        "tier": tier,
        "stt_model": stt_model,
        "llm_model": llm_model,
    }))
}
