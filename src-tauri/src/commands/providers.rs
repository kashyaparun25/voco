use tauri::State;
use crate::state::AppState;
use crate::providers::{ProviderConfig, ProviderRegistry, check_provider_health, fetch_provider_models};

#[tauri::command]
pub async fn get_providers(state: State<'_, AppState>) -> Result<Vec<ProviderConfig>, String> {
    let registry = ProviderRegistry::new(state.db.clone());
    registry.list_providers()
}

#[tauri::command]
pub async fn add_provider(state: State<'_, AppState>, config: ProviderConfig) -> Result<(), String> {
    let registry = ProviderRegistry::new(state.db.clone());
    registry.add_provider(config)
}

#[tauri::command]
pub async fn update_provider(state: State<'_, AppState>, config: ProviderConfig) -> Result<(), String> {
    if config.id.trim().is_empty() {
        return Err("Provider id cannot be empty".to_string());
    }
    let registry = ProviderRegistry::new(state.db.clone());
    registry.update_provider(config)
}

/// Lists available model ids for a provider. For embedded, returns downloaded models
/// from the model manager. For remote/local-server providers, queries their endpoint.
/// On failure returns an empty list (never errors the whole call).
#[tauri::command]
pub async fn list_provider_models(state: State<'_, AppState>, id: String) -> Result<Vec<String>, String> {
    if id.trim().is_empty() {
        return Err("Provider id cannot be empty".to_string());
    }

    if id == "embedded" {
        // Return downloaded models from the model manager.
        let models = state
            .model_manager
            .list_models()
            .into_iter()
            .filter(|m| m.is_downloaded)
            .map(|m| m.id)
            .collect();
        return Ok(models);
    }

    let registry = ProviderRegistry::new(state.db.clone());
    match registry.get_provider(&id)? {
        Some(config) => Ok(fetch_provider_models(&config).await),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn delete_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let registry = ProviderRegistry::new(state.db.clone());
    registry.delete_provider(&id)
}

#[tauri::command]
pub async fn set_active_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let registry = ProviderRegistry::new(state.db.clone());
    registry.set_active_provider(&id)
}

/// Health-check a provider config AS PROVIDED (possibly unsaved) — lets the
/// form's "Test connection" reflect exactly what the user has typed, instead
/// of the last-saved values.
#[tauri::command]
pub async fn test_provider_config(config: ProviderConfig) -> Result<bool, String> {
    let mut c = config;
    c.normalize_url();
    let status = check_provider_health(&c).await;
    if let Some(err) = status.error_message.filter(|_| !status.is_reachable) {
        return Err(err);
    }
    Ok(status.is_reachable && status.is_authenticated)
}

#[tauri::command]
pub async fn test_provider_connection(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let registry = ProviderRegistry::new(state.db.clone());
    if let Some(config) = registry.get_provider(&id)? {
        let status = check_provider_health(&config).await;
        Ok(status.is_reachable && status.is_authenticated)
    } else {
        Err(format!("Provider {} not found", id))
    }
}
