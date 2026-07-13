use tauri::{State, Emitter};
use crate::state::AppState;
use crate::commands::meeting::Segment;
use crate::storage::Database;
use log::{info, warn};
use serde_json::json;

/// Helper function to retrieve all transcript segments for a meeting from the database.
fn get_segments(db: &Database, meeting_id: &str) -> Result<Vec<Segment>, String> {
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

/// The request itself is too big for the provider/model (context or per-request
/// token cap). Retrying won't help — this must trigger map-reduce chunking.
fn is_too_large(e: &str) -> bool {
    let e = e.to_lowercase();
    e.contains("413")
        || e.contains("too large")
        || e.contains("reduce your message")
        || e.contains("context length")
        || e.contains("context_length")
        || e.contains("maximum context")
}

/// A transient rate limit (RPM/TPM/too-many-requests) worth backing off on.
/// NOT a too-large error (that needs chunking, not a retry).
fn is_transient_rate(e: &str) -> bool {
    if is_too_large(e) {
        return false;
    }
    let e = e.to_lowercase();
    e.contains("429")
        || e.contains("rate limit")
        || e.contains("rate_limit")
        || e.contains("too many")
        || e.contains("requests per minute")
        || e.contains("tokens per minute")
        || e.contains("tpm")
}

/// Non-streaming generate (via the streaming client, discarding tokens) with
/// backoff on transient rate limits. Used for the map step of map-reduce.
async fn generate_with_backoff(client: &crate::llm::client::ApiClient, prompt: &str) -> Result<String, String> {
    for attempt in 0..4u32 {
        match client.generate_stream(prompt, |_: &str| {}).await {
            Ok(s) => return Ok(s),
            Err(e) if attempt < 3 && is_transient_rate(&e) => {
                let wait = 15 * (attempt as u64 + 1);
                warn!("LLM chunk rate-limited (attempt {}): {} — retrying in {}s", attempt + 1, e, wait);
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err("LLM rate-limit retries exhausted".to_string())
}

/// Streaming generate to the UI (`summary-token` events) with backoff.
async fn stream_with_backoff(
    client: &crate::llm::client::ApiClient,
    prompt: &str,
    app: &tauri::AppHandle,
    meeting_id: &str,
) -> Result<String, String> {
    for attempt in 0..4u32 {
        let app2 = app.clone();
        let mid = meeting_id.to_string();
        let res = client
            .generate_stream(prompt, move |tok| {
                let _ = app2.emit("summary-token", json!({ "meeting_id": mid, "token": tok }));
            })
            .await;
        match res {
            Ok(s) => return Ok(s),
            Err(e) if attempt < 3 && is_transient_rate(&e) => {
                let wait = 15 * (attempt as u64 + 1);
                warn!("LLM summary rate-limited (attempt {}): {} — retrying in {}s", attempt + 1, e, wait);
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err("LLM rate-limit retries exhausted".to_string())
}

#[tauri::command]
pub async fn summarize_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    info!("Summarizing meeting: {}", meeting_id);
    
    // 1. Extract transcript segments from SQLite
    let segments = get_segments(&state.db, &meeting_id)?;
    if segments.is_empty() {
        return Err("Cannot summarize a meeting with no transcript segments.".to_string());
    }

    // 2. Format segments into a cohesive transcript text
    let formatted_transcript = crate::llm::prompt::format_transcript(&segments);

    // 3. Fetch requested length/style options from settings
    let (length, style) = crate::llm::prompt::get_summary_settings(&state.db);
    let template = crate::llm::prompt::get_summary_template(&state.db);

    // 4. Compile them into a cohesive prompt
    let prompt = crate::llm::prompt::generate_summary_prompt(&formatted_transcript, &length, &style, &template);

    // 5. Build/retrieve the chosen LLM engine
    let engine = crate::llm::get_llm_engine(&state.db)?;

    // 6. Execute the LLM engine to get the summary
    let summary = engine.generate(&prompt).await?;

    // 7. Save the resulting summary in the meetings table
    state.db.update_meeting_summary(&meeting_id, &summary)
        .map_err(|e| e.to_string())?;

    Ok(summary)
}

/// Streaming variant of `summarize_meeting`. Emits `summary-token` events as tokens
/// arrive and a final `summary-done` event with the complete text. Falls back to the
/// non-streaming path for embedded providers (which don't support SSE).
#[tauri::command]
pub async fn summarize_meeting_streaming(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    meeting_id: String,
    length: Option<String>,
    style: Option<String>,
) -> Result<String, String> {
    if meeting_id.trim().is_empty() {
        return Err("Meeting id cannot be empty".to_string());
    }
    info!("Streaming summary for meeting: {}", meeting_id);

    let segments = get_segments(&state.db, &meeting_id)?;
    if segments.is_empty() {
        return Err("Cannot summarize a meeting with no transcript segments.".to_string());
    }

    let formatted_transcript = crate::llm::prompt::format_transcript(&segments);

    // Use explicit args if given, else fall back to stored settings.
    let (default_length, default_style) = crate::llm::prompt::get_summary_settings(&state.db);
    let length = length.unwrap_or(default_length);
    let style = style.unwrap_or(default_style);
    let template = crate::llm::prompt::get_summary_template(&state.db);

    let prompt = crate::llm::prompt::generate_summary_prompt(&formatted_transcript, &length, &style, &template);

    // Determine provider: only remote providers support SSE streaming.
    let provider_id = state
        .db
        .get_setting("default_llm_provider")
        .unwrap_or(None)
        .unwrap_or_else(|| "embedded".to_string());

    let summary = if provider_id != "embedded" {
        let registry = crate::providers::ProviderRegistry::new(state.db.clone());
        match registry.get_provider(&provider_id)? {
            Some(config) => {
                let api_url = config.api_url.unwrap_or_default();
                let api_key = config.api_key.unwrap_or_default();
                // Per-task summary model, so one connection can do STT + LLM.
                let model = state.db
                    .get_setting("summary_llm_model")
                    .ok()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| if config.default_model.is_empty() { "default".to_string() } else { config.default_model });
                let client = crate::llm::client::ApiClient::new(api_url, api_key, model);

                // Adaptive: send the WHOLE transcript in one request first — a
                // capable provider/model (large context, paid tier) handles it
                // directly with no chunking. Only if the provider REJECTS it as
                // too large (e.g. Groq free-tier ~8k tokens/min → HTTP 413) do we
                // fall back to map-reduce: condense the transcript into notes in
                // chunks (repeating until it fits) then stream the final summary.
                match stream_with_backoff(&client, &prompt, &app_handle, &meeting_id).await {
                    Ok(s) => s,
                    Err(e) if is_too_large(&e) => {
                        info!("Summary: provider rejected the full request ({}); falling back to map-reduce", e);
                        const CHUNK_CHARS: usize = 16000; // ~4000 tokens/request
                        const CONDENSE_TARGET_TOKENS: usize = 4000;
                        let mut source = formatted_transcript.clone();
                        loop {
                            let chunks = crate::llm::prompt::chunk_transcript(&source, CHUNK_CHARS);
                            let _ = app_handle.emit("summary-token", json!({ "meeting_id": meeting_id, "token": format!("_(condensing {} sections of a long meeting…)_\n\n", chunks.len()) }));
                            let mut notes = String::new();
                            for (i, chunk) in chunks.iter().enumerate() {
                                let cp = crate::llm::prompt::chunk_notes_prompt(chunk, i + 1, chunks.len());
                                notes.push_str(generate_with_backoff(&client, &cp).await?.trim());
                                notes.push_str("\n\n");
                            }
                            source = notes;
                            // Stop once it fits one request (or can't be split further).
                            if chunks.len() <= 1 || crate::llm::prompt::estimate_tokens(&source) <= CONDENSE_TARGET_TOKENS {
                                break;
                            }
                        }
                        let fp = crate::llm::prompt::generate_summary_prompt(&source, &length, &style, &template);
                        stream_with_backoff(&client, &fp, &app_handle, &meeting_id).await?
                    }
                    Err(e) => return Err(e),
                }
            }
            None => {
                warn!("LLM provider '{}' not found; falling back to non-streaming engine", provider_id);
                let engine = crate::llm::get_llm_engine(&state.db)?;
                engine.generate(&prompt).await?
            }
        }
    } else {
        // Embedded engine: no SSE. Generate whole result, then emit as one token.
        let engine = crate::llm::get_llm_engine(&state.db)?;
        let result = engine.generate(&prompt).await?;
        let _ = app_handle.emit(
            "summary-token",
            json!({ "meeting_id": meeting_id, "token": result }),
        );
        result
    };

    state
        .db
        .update_meeting_summary(&meeting_id, &summary)
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit(
        "summary-done",
        json!({ "meeting_id": meeting_id, "summary": summary }),
    );

    Ok(summary)
}

#[tauri::command]
pub async fn regenerate_summary(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    info!("Regenerating summary for meeting: {}", meeting_id);
    summarize_meeting(state, meeting_id).await
}
