//! MCP tool catalog and dispatch. Tool results are returned as pretty-printed
//! JSON text (universally consumable by agents); the transcript tool returns the
//! rendered transcript directly when a text/markdown format is requested.

use crate::db::Db;
use serde_json::{json, Value};

/// The `tools/list` payload.
pub fn tool_list() -> Value {
    json!({ "tools": [
        {
            "name": "get_status",
            "description": "Health check and inventory: whether the MCP server is enabled in Voco, the database location, and how many meetings and dictations are visible. Call this first to verify the connection.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "list_meetings",
            "description": "List recorded and imported meetings, newest first, with title, date, duration, speakers, and whether a summary/notes exist. Use before drilling into a specific meeting.",
            "inputSchema": { "type": "object", "properties": {
                "limit": { "type": "integer", "description": "Max meetings (default 20, max 100)" },
                "source": { "type": "string", "enum": ["recording", "import", "all"], "description": "Filter by origin (default all)" },
                "from": { "type": "string", "description": "Only meetings on/after this ISO date (e.g. 2026-07-01)" },
                "to": { "type": "string", "description": "Only meetings on/before this ISO date" },
                "query": { "type": "string", "description": "Case-insensitive substring match on the title" }
            }, "additionalProperties": false }
        },
        {
            "name": "get_meeting",
            "description": "Full metadata for one meeting: title, date, duration, summary, the user's own notes, speakers, and segment count. Does not include the transcript body — use get_transcript for that.",
            "inputSchema": { "type": "object", "properties": {
                "meeting_id": { "type": "string", "description": "Meeting id, or the literal 'latest' for the most recent meeting" }
            }, "required": ["meeting_id"], "additionalProperties": false }
        },
        {
            "name": "get_transcript",
            "description": "Speaker-labelled transcript of a meeting, paginated by character budget. A long meeting will be truncated — follow next_offset_s to fetch the next window.",
            "inputSchema": { "type": "object", "properties": {
                "meeting_id": { "type": "string", "description": "Meeting id, or 'latest'" },
                "format": { "type": "string", "enum": ["text", "json", "markdown"], "description": "Output format (default text)" },
                "offset_s": { "type": "number", "description": "Start at this timestamp in seconds (default 0). Use next_offset_s from a prior call to page." },
                "limit_chars": { "type": "integer", "description": "Approx max characters to return (default 20000, max 50000)" }
            }, "required": ["meeting_id"], "additionalProperties": false }
        },
        {
            "name": "search",
            "description": "Full-text substring search across meeting transcripts and/or dictation history. Returns matching segments with meeting title, timestamp, and speaker.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "Text to search for" },
                "scope": { "type": "string", "enum": ["meetings", "dictations", "all"], "description": "What to search (default all)" },
                "limit": { "type": "integer", "description": "Max hits (default 20, max 100)" }
            }, "required": ["query"], "additionalProperties": false }
        },
        {
            "name": "list_dictations",
            "description": "Recent quick-dictation entries (voice typed via Voco), newest first, each truncated in the list view. Use get_dictation for the full text.",
            "inputSchema": { "type": "object", "properties": {
                "limit": { "type": "integer", "description": "Max entries (default 20, max 200)" },
                "since": { "type": "string", "description": "Only dictations on/after this ISO date" },
                "app": { "type": "string", "description": "Only dictations captured while this app was frontmost (substring match)" }
            }, "additionalProperties": false }
        },
        {
            "name": "get_dictation",
            "description": "Full text and metadata of one dictation. Use meeting_id 'latest' style: pass id 'latest' for the most recent dictation (handy for 'put my last dictation into this file').",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "string", "description": "Dictation id, or the literal 'latest'" }
            }, "required": ["id"], "additionalProperties": false }
        },
        {
            "name": "get_dictionary",
            "description": "The user's custom dictionary (spoken/misheard form -> corrected text): names, acronyms, product spellings. Use it to spell project-specific terms the way the user does.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }
    ] })
}

fn arg_str<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}
fn arg_i64(args: &Value, k: &str) -> Option<i64> {
    args.get(k).and_then(|v| v.as_i64())
}
fn arg_f64(args: &Value, k: &str) -> Option<f64> {
    args.get(k).and_then(|v| v.as_f64())
}
fn clamp(v: i64, default: i64, max: i64) -> i64 {
    if v <= 0 {
        default
    } else {
        v.min(max)
    }
}

/// Dispatch a tool call. `Ok(text)` is the payload for a text content block;
/// `Err(text)` becomes an `isError: true` result.
pub fn call(db: &Db, name: &str, args: &Value) -> Result<String, String> {
    let pretty = |v: Value| serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
    match name {
        "list_meetings" => {
            let limit = clamp(arg_i64(args, "limit").unwrap_or(0), 20, 100);
            let v = db
                .list_meetings(
                    limit,
                    arg_str(args, "source"),
                    arg_str(args, "from"),
                    arg_str(args, "to"),
                    arg_str(args, "query"),
                )
                .map_err(|e| e.to_string())?;
            Ok(pretty(v))
        }
        "get_meeting" => {
            let id = arg_str(args, "meeting_id").ok_or("meeting_id is required")?;
            db.get_meeting(id).map(pretty).map_err(|e| e.to_string())
        }
        "get_transcript" => {
            let id = arg_str(args, "meeting_id").ok_or("meeting_id is required")?;
            let format = arg_str(args, "format").unwrap_or("text");
            let offset = arg_f64(args, "offset_s").unwrap_or(0.0).max(0.0);
            let limit_chars = clamp(arg_i64(args, "limit_chars").unwrap_or(0), 20_000, 50_000) as usize;
            let v = db
                .get_transcript(id, format, offset, limit_chars)
                .map_err(|e| e.to_string())?;
            // For text/markdown, return the transcript directly; JSON callers get the envelope.
            if format == "json" {
                Ok(pretty(v))
            } else {
                let body = v.get("transcript").and_then(|t| t.as_str()).unwrap_or("").to_string();
                let truncated = v.get("truncated").and_then(|b| b.as_bool()).unwrap_or(false);
                if truncated {
                    let next = v.get("next_offset_s").cloned().unwrap_or(Value::Null);
                    Ok(format!(
                        "{body}\n\n[truncated — call get_transcript again with offset_s={next} for more]"
                    ))
                } else {
                    Ok(body)
                }
            }
        }
        "search" => {
            let q = arg_str(args, "query").ok_or("query is required")?;
            let scope = arg_str(args, "scope").unwrap_or("all");
            let limit = clamp(arg_i64(args, "limit").unwrap_or(0), 20, 100);
            db.search(q, scope, limit).map(pretty).map_err(|e| e.to_string())
        }
        "list_dictations" => {
            let limit = clamp(arg_i64(args, "limit").unwrap_or(0), 20, 200);
            let v = db
                .list_dictations(limit, arg_str(args, "since"), arg_str(args, "app"))
                .map_err(|e| e.to_string())?;
            Ok(pretty(v))
        }
        "get_dictation" => {
            let id = arg_str(args, "id").ok_or("id is required")?;
            db.get_dictation(id).map(pretty).map_err(|e| e.to_string())
        }
        "get_dictionary" => db.get_dictionary().map(pretty).map_err(|e| e.to_string()),
        other => Err(format!("Unknown tool '{other}'")),
    }
}
