use crate::storage::Database;
use crate::providers::config::ProviderConfig;
use log::info;

pub struct ProviderRegistry {
    db: Database,
}

impl ProviderRegistry {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Initializes the registry, populating it with default engines if the settings do not exist
    pub fn initialize(&self) -> Result<(), String> {
        let existing = self.db.get_setting("providers").map_err(|e| e.to_string())?;
        let empty = existing
            .as_ref()
            .map(|s| s.trim().is_empty() || s.trim() == "[]")
            .unwrap_or(true);
        if empty {
            info!("Populating provider registry with defaults.");
            let defaults = Self::get_default_providers();
            self.save_providers(&defaults)?;
        }
        // Retire the old remote Audio8 `/asr` server preset — superseded by the
        // native embedded engine. Remove the stale provider and migrate any STT
        // settings that pointed at it to the embedded engine (same model id).
        self.retire_audio8_remote_provider()?;
        Ok(())
    }

    /// One-time (idempotent) cleanup of the deprecated remote `audio8` provider.
    fn retire_audio8_remote_provider(&self) -> Result<(), String> {
        let mut providers = self.list_providers()?;
        let before = providers.len();
        providers.retain(|p| !(p.id == "audio8" && p.provider_type == "audio8"));
        if providers.len() != before {
            self.save_providers(&providers)?;
            info!("Removed deprecated remote Audio8 (/asr) provider; native embedded engine supersedes it.");
        }
        // Migrate STT provider selections from the remote preset to embedded so
        // users keep using Audio8 via the native path (model id is unchanged).
        for key in ["default_stt_provider", "meeting_stt_provider", "dictation_stt_provider"] {
            if self.db.get_setting(key).map_err(|e| e.to_string())?.as_deref() == Some("audio8") {
                let _ = self.db.set_setting(key, "embedded");
                info!("Migrated {key} from remote audio8 → embedded.");
            }
        }
        Ok(())
    }

    /// Returns the default list of providers
    pub fn get_default_providers() -> Vec<ProviderConfig> {
        vec![
            ProviderConfig {
                id: "openai".to_string(),
                name: "OpenAI Whisper".to_string(),
                api_key: None,
                api_url: Some("https://api.openai.com/v1".to_string()),
                provider_type: "openai".to_string(),
                default_model: "whisper-1".to_string(),
                is_active: true,
                is_default: true,
            },
            ProviderConfig {
                id: "groq".to_string(),
                name: "Groq ASR".to_string(),
                api_key: None,
                api_url: Some("https://api.groq.com/openai/v1".to_string()),
                provider_type: "groq".to_string(),
                default_model: "whisper-large-v3".to_string(),
                is_active: false,
                is_default: true,
            },
            ProviderConfig {
                id: "nvidia".to_string(),
                name: "NVIDIA NIM ASR".to_string(),
                api_key: None,
                api_url: Some("https://integrate.api.nvidia.com/v1".to_string()),
                provider_type: "nvidia".to_string(),
                default_model: "nvidia/parakeet-ctc-1.1b".to_string(),
                is_active: false,
                is_default: true,
            },
            ProviderConfig {
                id: "ollama".to_string(),
                name: "Ollama / Local ASR".to_string(),
                api_key: None,
                api_url: Some("http://localhost:11434/api".to_string()),
                provider_type: "ollama".to_string(),
                default_model: "whisper".to_string(),
                is_active: false,
                is_default: true,
            },
            ProviderConfig {
                id: "lmstudio".to_string(),
                name: "LM Studio ASR".to_string(),
                api_key: None,
                api_url: Some("http://localhost:1234/v1".to_string()),
                provider_type: "openai".to_string(),
                default_model: "default".to_string(),
                is_active: false,
                is_default: true,
            },
        ]
    }

    /// Lists all providers with decrypted API keys
    pub fn list_providers(&self) -> Result<Vec<ProviderConfig>, String> {
        let providers_json = self.db.get_setting("providers")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "[]".to_string());
        
        let mut providers: Vec<ProviderConfig> = serde_json::from_str(&providers_json)
            .map_err(|e| e.to_string())?;

        for p in &mut providers {
            p.decrypt_key();
        }

        Ok(providers)
    }

    /// Gets a single provider by ID (decrypted)
    pub fn get_provider(&self, id: &str) -> Result<Option<ProviderConfig>, String> {
        let providers = self.list_providers()?;
        Ok(providers.into_iter().find(|p| p.id == id))
    }

    /// Adds or updates a provider (key is encrypted before storing)
    pub fn add_provider(&self, mut config: ProviderConfig) -> Result<(), String> {
        let mut providers = self.list_providers()?;

        // Canonicalize: users often paste a full endpoint; store the base URL.
        config.normalize_url();
        // Encrypt the key for storage
        config.encrypt_key();

        // Update if existing or append
        if let Some(pos) = providers.iter().position(|p| p.id == config.id) {
            providers[pos] = config;
        } else {
            providers.push(config);
        }

        self.save_providers(&providers)
    }

    /// Updates an existing provider by ID. Errors if the provider is not found.
    pub fn update_provider(&self, mut config: ProviderConfig) -> Result<(), String> {
        if config.id.trim().is_empty() {
            return Err("Provider id cannot be empty".to_string());
        }

        let mut providers = self.list_providers()?;

        let Some(pos) = providers.iter().position(|p| p.id == config.id) else {
            return Err(format!("Provider {} not found", config.id));
        };

        config.normalize_url();
        config.encrypt_key();
        providers[pos] = config;

        self.save_providers(&providers)
    }

    /// Deletes a provider by ID
    pub fn delete_provider(&self, id: &str) -> Result<(), String> {
        let mut providers = self.list_providers()?;
        providers.retain(|p| p.id != id);
        self.save_providers(&providers)
    }

    /// Sets a provider as the active one
    pub fn set_active_provider(&self, id: &str) -> Result<(), String> {
        let mut providers = self.list_providers()?;
        let mut found = false;
        
        for p in &mut providers {
            if p.id == id {
                p.is_active = true;
                found = true;
            } else {
                p.is_active = false;
            }
        }

        if !found {
            return Err(format!("Provider {} not found", id));
        }

        self.save_providers(&providers)
    }

    /// Gets the currently active provider (decrypted)
    pub fn get_active_provider(&self) -> Result<Option<ProviderConfig>, String> {
        let providers = self.list_providers()?;
        Ok(providers.into_iter().find(|p| p.is_active))
    }

    // Helper to serialize and save providers to the DB with encrypted keys
    fn save_providers(&self, providers: &[ProviderConfig]) -> Result<(), String> {
        let mut encrypted_providers = providers.to_vec();
        for p in &mut encrypted_providers {
            p.encrypt_key();
        }

        let json = serde_json::to_string(&encrypted_providers)
            .map_err(|e| e.to_string())?;

        self.db.set_setting("providers", &json)
            .map_err(|e| e.to_string())
    }
}
