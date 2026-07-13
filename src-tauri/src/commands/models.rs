use tauri::State;
use crate::state::AppState;
use crate::stt::{CustomModel, ModelInfo, ModelManager};
use crate::storage::Database;
use serde::{Deserialize, Serialize};

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    Ok(state.model_manager.list_models())
}

#[derive(Serialize, Deserialize, Clone)]
struct StoredCustomModel {
    id: String,
    name: String,
    filename: String,
    url: String,
    category: String,
    #[serde(default)]
    size_bytes: u64,
}

fn slugify(s: &str) -> String {
    let slug: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    slug.trim_matches('-').to_string()
}

fn guess_category(filename: &str) -> String {
    let f = filename.to_lowercase();
    if f.ends_with(".gguf") {
        "llm".to_string()
    } else if f.contains("vad") || f.contains("silero") {
        "vad".to_string()
    } else {
        "stt".to_string()
    }
}

/// Load previously-added custom models from the DB into the model manager.
/// Call once at startup.
pub fn load_custom_models(db: &Database, mm: &ModelManager) {
    if let Ok(Some(json)) = db.get_setting("custom_models") {
        if let Ok(list) = serde_json::from_str::<Vec<StoredCustomModel>>(&json) {
            for m in list {
                mm.register_custom_model(CustomModel {
                    id: m.id,
                    name: m.name,
                    filename: m.filename,
                    url: m.url,
                    size_bytes: m.size_bytes,
                    category: m.category,
                });
            }
        }
    }
}

fn persist_custom(db: &Database, entry: &StoredCustomModel) -> Result<(), String> {
    let mut list: Vec<StoredCustomModel> = db
        .get_setting("custom_models")
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    list.retain(|m| m.id != entry.id);
    list.push(entry.clone());
    let json = serde_json::to_string(&list).map_err(|e| e.to_string())?;
    db.set_setting("custom_models", &json).map_err(|e| e.to_string())
}

/// Add a model by an arbitrary download URL (GGUF/ONNX/ggml), then download it.
/// Returns the generated model id (usable as an STT/LLM model everywhere).
#[tauri::command]
pub async fn add_custom_model(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    name: String,
    url: String,
    category: Option<String>,
) -> Result<String, String> {
    let url = url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Please provide a valid http(s) URL".to_string());
    }
    let name = if name.trim().is_empty() { "Custom Model".to_string() } else { name.trim().to_string() };

    // Derive the filename from the URL (strip any query string).
    let filename = url
        .rsplit('/')
        .next()
        .and_then(|s| s.split('?').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("custom-model.bin")
        .to_string();
    let category = category
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| guess_category(&filename));
    let id = format!("custom-{}", slugify(&name));

    let entry = StoredCustomModel {
        id: id.clone(),
        name: name.clone(),
        filename: filename.clone(),
        url: url.clone(),
        category: category.clone(),
        size_bytes: 0,
    };

    state.model_manager.register_custom_model(CustomModel {
        id: id.clone(),
        name,
        filename,
        url,
        size_bytes: 0,
        category,
    });
    persist_custom(&state.db, &entry)?;

    // Kick off the download (returns after the background task is spawned).
    state
        .model_manager
        .download_model_with_progress(&id, app_handle)
        .await
        .map_err(|e| e.to_string())?;

    Ok(id)
}

#[tauri::command]
pub async fn download_model(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("Model id cannot be empty".to_string());
    }
    state
        .model_manager
        .download_model_with_progress(&id, app_handle)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.model_manager.delete_model(&id).map_err(|e| e.to_string())
}
