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
    let structure = crate::llm::prompt::resolve_template_structure(&state.db, &template);
    let prompt = crate::llm::prompt::generate_summary_prompt_with_structure(&formatted_transcript, &length, &style, &structure);

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

    let structure = crate::llm::prompt::resolve_template_structure(&state.db, &template);
    let prompt = crate::llm::prompt::generate_summary_prompt_with_structure(&formatted_transcript, &length, &style, &structure);

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
                        let fp = crate::llm::prompt::generate_summary_prompt_with_structure(&source, &length, &style, &structure);
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

    // Granola-style auto-title: a short descriptive subtitle generated from
    // the fresh notes. Stored as a setting (never overwrites the meeting's
    // own name) and announced to the UI.
    generate_ai_title(&state, &app_handle, &meeting_id, &summary).await;

    Ok(summary)
}

/// Generate and persist a 4–9 word descriptive title for a meeting from its
/// summary. Failures are logged, never surfaced — this is a nicety.
async fn generate_ai_title(
    state: &State<'_, AppState>,
    app_handle: &tauri::AppHandle,
    meeting_id: &str,
    summary: &str,
) {
    // Cap the excerpt on a char boundary.
    let mut end = summary.len().min(4_000);
    while end < summary.len() && !summary.is_char_boundary(end) {
        end += 1;
    }
    let prompt = format!(
        "Write a concise, descriptive title (4-9 words) for this meeting based \
         on the notes below. Capture the actual subject, not generic phrasing. \
         Return ONLY the title — no quotes, no trailing punctuation.\n\nNOTES:\n{}",
        &summary[..end]
    );
    let engine = match crate::llm::get_llm_engine(&state.db) {
        Ok(e) => e,
        Err(e) => {
            warn!("AI title skipped (no LLM engine): {}", e);
            return;
        }
    };
    match engine.generate(&prompt).await {
        Ok(raw) => {
            let title = raw.trim().trim_matches('"').trim().to_string();
            // Guard against the model rambling: keep only single-line, sane-length titles.
            if !title.is_empty() && title.len() <= 120 && !title.contains('\n') {
                let _ = state.db.set_setting(&format!("ai_title::{}", meeting_id), &title);
                let _ = app_handle.emit(
                    "meeting-title-suggested",
                    json!({ "meeting_id": meeting_id, "title": title }),
                );
                info!("AI title for {}: {}", meeting_id, title);
            }
        }
        Err(e) => warn!("AI title generation failed: {}", e),
    }
}

#[tauri::command]
pub async fn regenerate_summary(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    info!("Regenerating summary for meeting: {}", meeting_id);
    summarize_meeting(state, meeting_id).await
}

/// Suggest a concise descriptive title for a meeting (from its notes when
/// they exist, else the transcript). Returns the suggestion WITHOUT saving —
/// the frontend applies it via `rename_meeting` so the user stays in control.
#[tauri::command]
pub async fn suggest_meeting_title(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    let summary: Option<String> = state
        .db
        .conn()
        .query_row("SELECT summary FROM meetings WHERE id = ?1", [meeting_id.as_str()], |r| r.get(0))
        .ok()
        .flatten();

    let source = match summary {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            let segments = get_segments(&state.db, &meeting_id)?;
            if segments.is_empty() {
                return Err("This meeting has no transcript yet.".to_string());
            }
            crate::llm::prompt::format_transcript(&segments)
        }
    };
    let mut end = source.len().min(6_000);
    while end < source.len() && !source.is_char_boundary(end) {
        end += 1;
    }
    let prompt = format!(
        "Write a concise, descriptive title (3-8 words) for this meeting. \
         Capture the actual subject, not generic phrasing. Return ONLY the \
         title — no quotes, no trailing punctuation.\n\n{}",
        &source[..end]
    );
    let engine = crate::llm::get_llm_engine(&state.db)?;
    let raw = engine.generate(&prompt).await?;
    let title = raw.trim().trim_matches('"').trim().to_string();
    if title.is_empty() || title.len() > 120 || title.contains('\n') {
        return Err("Could not generate a usable title.".to_string());
    }
    Ok(title)
}

/// Granola-style "Ask anything": answer a free-form question with the same
/// LLM that generates meeting summaries. With a `meeting_id`, the context is
/// that meeting's transcript (+ stored summary); without one (home page), the
/// context is the titles + summaries of the most recent meetings. Emits the
/// answer via `chat-answer` (with the echoed `request_id`) and also returns it.
#[tauri::command]
pub async fn ask_meeting_ai(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    meeting_id: Option<String>,
    question: String,
    request_id: String,
) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("Question is empty".to_string());
    }

    // Keep the context inside a conservative token budget: transcripts are
    // tail-truncated (the recent discussion answers most questions), and the
    // home context caps at the 5 newest meetings.
    const MAX_CONTEXT_CHARS: usize = 24_000;

    let context = match &meeting_id {
        Some(mid) => {
            let segments = get_segments(&state.db, mid)?;
            if segments.is_empty() {
                return Err("This meeting has no transcript yet.".to_string());
            }
            let mut transcript = crate::llm::prompt::format_transcript(&segments);
            if transcript.len() > MAX_CONTEXT_CHARS {
                let cut = transcript.len() - MAX_CONTEXT_CHARS;
                // Truncate on a char boundary.
                let cut = (cut..transcript.len())
                    .find(|&i| transcript.is_char_boundary(i))
                    .unwrap_or(cut);
                transcript = format!("[…earlier discussion omitted…]\n{}", &transcript[cut..]);
            }
            let summary: Option<String> = state
                .db
                .conn()
                .query_row("SELECT summary FROM meetings WHERE id = ?1", [mid.as_str()], |r| r.get(0))
                .ok()
                .flatten();
            match summary {
                Some(s) if !s.trim().is_empty() => {
                    format!("MEETING NOTES:\n{}\n\nMEETING TRANSCRIPT:\n{}", s, transcript)
                }
                _ => format!("MEETING TRANSCRIPT:\n{}", transcript),
            }
        }
        None => {
            let conn = state.db.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT title, created_at, summary FROM meetings \
                     WHERE summary IS NOT NULL AND summary != '' \
                     ORDER BY created_at DESC LIMIT 5",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            let mut ctx = String::new();
            for r in rows.flatten() {
                let (title, created, summary) = r;
                ctx.push_str(&format!("MEETING \"{}\" ({}):\n{}\n\n", title, created, summary));
                if ctx.len() > MAX_CONTEXT_CHARS {
                    break;
                }
            }
            if ctx.is_empty() {
                return Err("No meeting notes yet — record or import a meeting first.".to_string());
            }
            ctx
        }
    };

    let prompt = format!(
        "You are the meeting assistant inside the Voco app. Answer the user's \
         question using ONLY the meeting context below. Be direct and concise; \
         use short Markdown (bullets/bold) when it helps. If the context does \
         not contain the answer, say so plainly.\n\n{}\n\nQUESTION: {}",
        context, question
    );

    let engine = crate::llm::get_llm_engine(&state.db)?;
    let answer = engine.generate(&prompt).await?;

    let _ = app_handle.emit(
        "chat-answer",
        json!({ "request_id": request_id, "meeting_id": meeting_id, "answer": answer }),
    );
    Ok(answer)
}
