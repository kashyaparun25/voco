use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;
use serde::Serialize;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tauri::Emitter;
use log::{info, error};

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub is_downloaded: bool,
    pub progress: f32, // Download progress 0.0 to 1.0
    pub category: String, // "stt" or "llm"
}

struct SupportedModel {
    id: &'static str,
    name: &'static str,
    filename: &'static str,
    url: &'static str,
    size_bytes: u64,
    category: &'static str,
}

const SUPPORTED_MODELS: &[SupportedModel] = &[
    SupportedModel {
        id: "whisper-tiny-q5",
        name: "Whisper Tiny (Embedded)",
        filename: "ggml-tiny.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        size_bytes: 75_000_000,
        category: "stt",
    },
    SupportedModel {
        id: "whisper-base-q5",
        name: "Whisper Base (Embedded)",
        filename: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size_bytes: 140_000_000,
        category: "stt",
    },
    SupportedModel {
        id: "whisper-small-q5",
        name: "Whisper Small (Embedded)",
        filename: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size_bytes: 465_000_000,
        category: "stt",
    },
    SupportedModel {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo (Embedded)",
        filename: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        size_bytes: 810_000_000,
        category: "stt",
    },
    SupportedModel {
        id: "distil-whisper-large-v3",
        name: "Distil Whisper Large v3 (Embedded)",
        filename: "ggml-distil-large-v3.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-distil-large-v3.bin",
        size_bytes: 930_000_000,
        category: "stt",
    },
    SupportedModel {
        id: "distil-whisper-medium-en",
        name: "Distil Whisper Medium English (Embedded)",
        filename: "ggml-distil-medium.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-distil-medium.en.bin",
        size_bytes: 390_000_000,
        category: "stt",
    },
    SupportedModel {
        id: "qwen-1.5b-instruct-q4",
        name: "Qwen 1.5B Chat (Embedded LLM)",
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        size_bytes: 980_000_000,
        category: "llm",
    },
    SupportedModel {
        id: "qwen-0.5b-instruct-q4",
        name: "Qwen 0.5B Chat (Nano Embedded LLM)",
        filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
        size_bytes: 400_000_000,
        category: "llm",
    },
    SupportedModel {
        id: "silero-vad",
        name: "Silero VAD (Neural Voice Detection)",
        filename: "silero_vad.onnx",
        url: "https://raw.githubusercontent.com/snakers4/silero-vad/master/src/silero_vad/data/silero_vad.onnx",
        size_bytes: 2_300_000,
        category: "vad",
    },
];

/// One file within a multi-file (bundle) model.
struct BundleFile {
    url: &'static str,
    /// Filename to save as inside the bundle directory.
    dest: &'static str,
}

/// A model made of several files downloaded into a subdirectory (e.g. the
/// Parakeet ONNX export: encoder + decoder_joint + vocab).
struct BundleModel {
    id: &'static str,
    name: &'static str,
    /// Subdirectory under models_dir where the files live.
    subdir: &'static str,
    category: &'static str,
    size_bytes: u64,
    files: &'static [BundleFile],
}

const SUPPORTED_BUNDLES: &[BundleModel] = &[BundleModel {
    // NeMo Parakeet TDT 0.6B v3 (multilingual), int8 ONNX export by istupakov.
    // We save the int8 files under the base names the ParakeetEngine probes for
    // (encoder-model.onnx / decoder_joint-model.onnx), in the dir it expects.
    id: "parakeet-tdt-v3",
    name: "Parakeet TDT 0.6B v3 (ONNX)",
    subdir: "parakeet-tdt-0.6b",
    category: "stt",
    size_bytes: 660_000_000,
    files: &[
        BundleFile {
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/encoder-model.int8.onnx",
            dest: "encoder-model.onnx",
        },
        BundleFile {
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/decoder_joint-model.int8.onnx",
            dest: "decoder_joint-model.onnx",
        },
        BundleFile {
            url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/vocab.txt",
            dest: "vocab.txt",
        },
    ],
}, BundleModel {
    // Audio8-ASR 0.1B (arkasr speech-LLM), int8 ONNX. Native embedded engine —
    // downloads the int8 subset (~0.9GB) on demand; layout mirrors model_bundle/
    // so Audio8Engine::new can read graphs+json at root and weights/ underneath.
    id: "audio8-asr-0.1b",
    name: "Audio8-ASR 0.1B (ONNX, 7 lang incl. Cantonese)",
    subdir: "audio8-asr-0.1b",
    category: "stt",
    size_bytes: 870_000_000,
    files: &[
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/audio_hidden_int8.onnx", dest: "audio_hidden_int8.onnx" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/lm_cache_prefill_int8.onnx", dest: "lm_cache_prefill_int8.onnx" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/lm_cache_prefill_int8.onnx.data", dest: "lm_cache_prefill_int8.onnx.data" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/lm_cache_decode_int8.onnx", dest: "lm_cache_decode_int8.onnx" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/lm_cache_decode_int8.onnx.data", dest: "lm_cache_decode_int8.onnx.data" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/metadata.json", dest: "metadata.json" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/tokenizer.json", dest: "tokenizer.json" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/weights/token_embedding.npy", dest: "weights/token_embedding.npy" },
        BundleFile { url: "https://huggingface.co/AutoArk-AI/Audio8-ASR-0.1B-onnx-runtime/resolve/main/model_bundle/weights/audio_projector.npz", dest: "weights/audio_projector.npz" },
    ],
}];

/// A user-added model pointing at an arbitrary download URL.
#[derive(Debug, Clone)]
pub struct CustomModel {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    pub category: String,
}

#[derive(Clone)]
pub struct ModelManager {
    models_dir: PathBuf,
    active_downloads: Arc<Mutex<HashMap<String, f32>>>,
    custom: Arc<Mutex<HashMap<String, CustomModel>>>,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        // Ensure directory exists
        if !models_dir.exists() {
            let _ = std::fs::create_dir_all(&models_dir);
        }

        Self {
            models_dir,
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            custom: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a custom model (arbitrary download URL). Idempotent by id.
    pub fn register_custom_model(&self, m: CustomModel) {
        self.custom.lock().insert(m.id.clone(), m);
    }

    /// Resolve a model id to `(filename, url, size_bytes, name, category)`,
    /// checking built-in models first, then custom ones.
    fn resolve(&self, id: &str) -> Option<(String, String, u64, String, String)> {
        if let Some(m) = SUPPORTED_MODELS.iter().find(|m| m.id == id) {
            return Some((
                m.filename.to_string(),
                m.url.to_string(),
                m.size_bytes,
                m.name.to_string(),
                m.category.to_string(),
            ));
        }
        self.custom.lock().get(id).map(|m| {
            (
                m.filename.clone(),
                m.url.clone(),
                m.size_bytes,
                m.name.clone(),
                m.category.clone(),
            )
        })
    }

    fn find_bundle(id: &str) -> Option<&'static BundleModel> {
        SUPPORTED_BUNDLES.iter().find(|b| b.id == id)
    }

    /// True when every file of a bundle model is present and non-empty.
    fn bundle_downloaded(&self, b: &BundleModel) -> bool {
        let dir = self.models_dir.join(b.subdir);
        b.files.iter().all(|f| {
            let p = dir.join(f.dest);
            p.exists() && p.metadata().map(|m| m.len() > 0).unwrap_or(false)
        })
    }

    /// The directory where models are stored on disk.
    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }

    pub fn list_models(&self) -> Vec<ModelInfo> {
        let active = self.active_downloads.lock();
        let mut out: Vec<ModelInfo> = SUPPORTED_MODELS.iter().map(|m| {
            let path = self.models_dir.join(m.filename);
            let is_downloaded = path.exists() && path.metadata().map(|meta| meta.len() > 0).unwrap_or(false);
            let progress = if is_downloaded {
                1.0
            } else {
                *active.get(m.id).unwrap_or(&0.0)
            };

            ModelInfo {
                id: m.id.to_string(),
                name: m.name.to_string(),
                size_bytes: m.size_bytes,
                is_downloaded: is_downloaded && progress >= 1.0,
                progress,
                category: m.category.to_string(),
            }
        }).collect();

        // Append user-added custom models.
        for m in self.custom.lock().values() {
            let path = self.models_dir.join(&m.filename);
            let is_downloaded = path.exists() && path.metadata().map(|meta| meta.len() > 0).unwrap_or(false);
            let progress = if is_downloaded { 1.0 } else { *active.get(&m.id).unwrap_or(&0.0) };
            out.push(ModelInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                size_bytes: m.size_bytes,
                is_downloaded: is_downloaded && progress >= 1.0,
                progress,
                category: m.category.clone(),
            });
        }

        // Append multi-file bundle models (e.g. Parakeet ONNX).
        for b in SUPPORTED_BUNDLES {
            let is_downloaded = self.bundle_downloaded(b);
            let progress = if is_downloaded { 1.0 } else { *active.get(b.id).unwrap_or(&0.0) };
            out.push(ModelInfo {
                id: b.id.to_string(),
                name: b.name.to_string(),
                size_bytes: b.size_bytes,
                is_downloaded: is_downloaded && progress >= 1.0,
                progress,
                category: b.category.to_string(),
            });
        }
        out
    }

    pub fn get_model_path(&self, id: &str) -> Option<PathBuf> {
        let (filename, ..) = self.resolve(id)?;
        let path = self.models_dir.join(filename);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    pub async fn download_model(&self, id: &str) -> anyhow::Result<()> {
        self.download_model_inner(id, None::<tauri::AppHandle>).await
    }

    /// Same as `download_model`, but emits `model-download-progress` events on the given app handle.
    pub async fn download_model_with_progress(
        &self,
        id: &str,
        app_handle: tauri::AppHandle,
    ) -> anyhow::Result<()> {
        self.download_model_inner(id, Some(app_handle)).await
    }

    async fn download_model_inner(
        &self,
        id: &str,
        app_handle: Option<tauri::AppHandle>,
    ) -> anyhow::Result<()> {
        // Multi-file bundle models (Parakeet ONNX) take a separate path.
        if let Some(bundle) = Self::find_bundle(id) {
            return self.download_bundle_inner(bundle, app_handle).await;
        }

        let (filename, url, size_bytes, ..) = self
            .resolve(id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", id))?;

        let dest_path = self.models_dir.join(&filename);

        // Check if already downloaded
        if dest_path.exists() {
            info!("Model {} already downloaded.", id);
            return Ok(());
        }

        // Check if download is already in progress
        {
            let mut active = self.active_downloads.lock();
            if active.contains_key(id) {
                return Ok(());
            }
            active.insert(id.to_string(), 0.0);
        }

        let active_downloads_clone = self.active_downloads.clone();
        let id_str = id.to_string();
        let url_str = url;
        let known_size = size_bytes;

        tauri::async_runtime::spawn(async move {
            info!("Starting download for model {} from {}", id_str, url_str);
            let client = reqwest::Client::new();
            
            let res = match client.get(&url_str).send().await {
                Ok(r) => r,
                Err(e) => {
                    error!("Failed to request model {}: {:?}", id_str, e);
                    active_downloads_clone.lock().remove(&id_str);
                    return;
                }
            };

            let total_size = res.content_length().unwrap_or(0);
            
            let temp_dest = dest_path.with_extension("download");
            let mut file = match File::create(&temp_dest).await {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to create temp file for model {}: {:?}", id_str, e);
                    active_downloads_clone.lock().remove(&id_str);
                    return;
                }
            };

            // Fall back to the known model size if the server doesn't report content-length.
            let effective_total = if total_size > 0 { total_size } else { known_size };

            let mut downloaded: u64 = 0;
            let mut last_reported = 0.0f32;
            let mut last_emitted_bytes: u64 = 0;
            let mut response = res;

            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if let Err(e) = file.write_all(&chunk).await {
                            error!("Failed to write chunk for model {}: {:?}", id_str, e);
                            active_downloads_clone.lock().remove(&id_str);
                            let _ = tokio::fs::remove_file(&temp_dest).await;
                            return;
                        }

                        downloaded += chunk.len() as u64;
                        if total_size > 0 {
                            let progress = downloaded as f32 / total_size as f32;
                            if progress - last_reported > 0.01 || progress >= 1.0 {
                                active_downloads_clone.lock().insert(id_str.clone(), progress);
                                last_reported = progress;
                            }
                        }

                        // Emit throttled progress events (~every 512KB or 2%).
                        if let Some(handle) = &app_handle {
                            let percent = if effective_total > 0 {
                                (downloaded as f32 / effective_total as f32 * 100.0).min(100.0)
                            } else {
                                0.0
                            };
                            let by_bytes = downloaded.saturating_sub(last_emitted_bytes) >= 512 * 1024;
                            let by_percent = percent - (last_emitted_bytes as f32
                                / effective_total.max(1) as f32
                                * 100.0)
                                >= 2.0;
                            if by_bytes || by_percent {
                                let _ = handle.emit(
                                    "model-download-progress",
                                    serde_json::json!({
                                        "model_id": id_str,
                                        "downloaded_bytes": downloaded,
                                        "total_bytes": effective_total,
                                        "percent": percent,
                                    }),
                                );
                                last_emitted_bytes = downloaded;
                            }
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(e) => {
                        error!("Error during download stream of model {}: {:?}", id_str, e);
                        active_downloads_clone.lock().remove(&id_str);
                        let _ = tokio::fs::remove_file(&temp_dest).await;
                        return;
                    }
                }
            }

            // Sync and rename
            if let Err(e) = file.sync_all().await {
                error!("Failed to sync model file {}: {:?}", id_str, e);
                active_downloads_clone.lock().remove(&id_str);
                let _ = tokio::fs::remove_file(&temp_dest).await;
                return;
            }
            drop(file);

            if let Err(e) = tokio::fs::rename(&temp_dest, &dest_path).await {
                error!("Failed to rename temp file for model {}: {:?}", id_str, e);
                active_downloads_clone.lock().remove(&id_str);
                let _ = tokio::fs::remove_file(&temp_dest).await;
                return;
            }

            active_downloads_clone.lock().insert(id_str.clone(), 1.0);
            if let Some(handle) = &app_handle {
                let _ = handle.emit(
                    "model-download-progress",
                    serde_json::json!({
                        "model_id": id_str,
                        "downloaded_bytes": downloaded,
                        "total_bytes": if effective_total > 0 { effective_total } else { downloaded },
                        "percent": 100.0,
                    }),
                );
            }
            info!("Model {} download completed successfully.", id_str);
        });

        Ok(())
    }

    /// Download a multi-file bundle model into its subdirectory, saving each
    /// file under its target name. Emits combined progress on `model_id`.
    async fn download_bundle_inner(
        &self,
        bundle: &'static BundleModel,
        app_handle: Option<tauri::AppHandle>,
    ) -> anyhow::Result<()> {
        if self.bundle_downloaded(bundle) {
            info!("Bundle model {} already downloaded.", bundle.id);
            return Ok(());
        }
        {
            let mut active = self.active_downloads.lock();
            if active.contains_key(bundle.id) {
                return Ok(());
            }
            active.insert(bundle.id.to_string(), 0.0);
        }

        let dir = self.models_dir.join(bundle.subdir);
        let active = self.active_downloads.clone();
        let id = bundle.id.to_string();
        let files = bundle.files;
        let n = files.len().max(1);

        tauri::async_runtime::spawn(async move {
            let _ = tokio::fs::create_dir_all(&dir).await;
            let client = reqwest::Client::new();

            for (i, f) in files.iter().enumerate() {
                let dest_path = dir.join(f.dest);
                if dest_path.exists() && dest_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                    continue; // already have this file
                }
                // dest may include a subdir (e.g. "weights/token_embedding.npy").
                if let Some(parent) = dest_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let temp = dest_path.with_extension("download");

                let res = match client.get(f.url).send().await {
                    Ok(r) if r.status().is_success() => r,
                    Ok(r) => {
                        error!("Bundle {} file {} HTTP {}", id, f.dest, r.status());
                        active.lock().remove(&id);
                        let _ = tokio::fs::remove_file(&temp).await;
                        return;
                    }
                    Err(e) => {
                        error!("Bundle {} request failed: {:?}", id, e);
                        active.lock().remove(&id);
                        return;
                    }
                };
                let total = res.content_length().unwrap_or(0);
                let mut file = match File::create(&temp).await {
                    Ok(x) => x,
                    Err(e) => {
                        error!("Bundle {} temp create failed: {:?}", id, e);
                        active.lock().remove(&id);
                        return;
                    }
                };

                let mut dl: u64 = 0;
                let mut last = -1.0f32;
                let mut resp = res;
                loop {
                    match resp.chunk().await {
                        Ok(Some(c)) => {
                            if file.write_all(&c).await.is_err() {
                                active.lock().remove(&id);
                                let _ = tokio::fs::remove_file(&temp).await;
                                return;
                            }
                            dl += c.len() as u64;
                            let frac = if total > 0 { dl as f32 / total as f32 } else { 0.0 };
                            let overall = ((i as f32) + frac) / n as f32;
                            if overall - last > 0.01 {
                                active.lock().insert(id.clone(), overall);
                                last = overall;
                                if let Some(h) = &app_handle {
                                    let _ = h.emit(
                                        "model-download-progress",
                                        serde_json::json!({ "model_id": id, "percent": overall * 100.0 }),
                                    );
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Bundle {} stream error: {:?}", id, e);
                            active.lock().remove(&id);
                            let _ = tokio::fs::remove_file(&temp).await;
                            return;
                        }
                    }
                }
                let _ = file.sync_all().await;
                drop(file);
                if tokio::fs::rename(&temp, &dest_path).await.is_err() {
                    active.lock().remove(&id);
                    return;
                }
            }

            active.lock().insert(id.clone(), 1.0);
            if let Some(h) = &app_handle {
                let _ = h.emit("model-download-progress", serde_json::json!({ "model_id": id, "percent": 100.0 }));
                let _ = h.emit("model-download-complete", serde_json::json!({ "model_id": id }));
            }
            info!("Bundle model {} downloaded successfully.", id);
        });

        Ok(())
    }

    pub fn delete_model(&self, id: &str) -> anyhow::Result<()> {
        // Bundle models: remove the whole subdirectory.
        if let Some(b) = Self::find_bundle(id) {
            let dir = self.models_dir.join(b.subdir);
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            self.active_downloads.lock().remove(id);
            return Ok(());
        }

        let (filename, ..) = self
            .resolve(id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", id))?;

        let dest_path = self.models_dir.join(filename);
        if dest_path.exists() {
            std::fs::remove_file(dest_path)?;
        }

        let mut active = self.active_downloads.lock();
        active.remove(id);

        Ok(())
    }
}
