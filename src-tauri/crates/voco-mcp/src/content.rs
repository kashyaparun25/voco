//! MCP resources and prompts. Resources let clients attach Voco content by URI
//! (e.g. Claude Code `@`-mentions); prompts are one-shot templates that inline
//! meeting context.

use crate::db::Db;
use crate::tools;
use serde_json::{json, Value};

pub fn resource_list() -> Value {
    json!({ "resources": [
        {
            "uri": "voco://meetings/recent",
            "name": "Recent meetings",
            "description": "The 20 most recent meetings with titles, dates, and speakers.",
            "mimeType": "application/json"
        },
        {
            "uri": "voco://dictations/recent",
            "name": "Recent dictations",
            "description": "The 20 most recent quick-dictation entries.",
            "mimeType": "application/json"
        }
    ] })
}

pub fn resource_templates_list() -> Value {
    json!({ "resourceTemplates": [
        {
            "uriTemplate": "voco://meeting/{id}",
            "name": "Meeting details",
            "description": "Metadata, summary, and notes for a meeting.",
            "mimeType": "application/json"
        },
        {
            "uriTemplate": "voco://meeting/{id}/transcript",
            "name": "Meeting transcript",
            "description": "Speaker-labelled transcript of a meeting (first window).",
            "mimeType": "text/plain"
        }
    ] })
}

pub fn resource_read(db: &Db, uri: &str) -> Result<Value, String> {
    let rest = uri
        .strip_prefix("voco://")
        .ok_or_else(|| format!("Unsupported URI scheme: {uri}"))?;

    let (mime, text) = match rest {
        "meetings/recent" => (
            "application/json",
            tools::call(db, "list_meetings", &json!({}))?,
        ),
        "dictations/recent" => (
            "application/json",
            tools::call(db, "list_dictations", &json!({}))?,
        ),
        path if path.starts_with("meeting/") => {
            let tail = &path["meeting/".len()..];
            if let Some(id) = tail.strip_suffix("/transcript") {
                (
                    "text/plain",
                    tools::call(db, "get_transcript", &json!({ "meeting_id": id }))?,
                )
            } else {
                (
                    "application/json",
                    tools::call(db, "get_meeting", &json!({ "meeting_id": tail }))?,
                )
            }
        }
        _ => return Err(format!("Unknown resource: {uri}")),
    };

    Ok(json!({ "contents": [ { "uri": uri, "mimeType": mime, "text": text } ] }))
}

pub fn prompt_list() -> Value {
    json!({ "prompts": [
        {
            "name": "meeting-context",
            "description": "Load a meeting's summary and transcript as context for your next request.",
            "arguments": [
                { "name": "meeting_id", "description": "Meeting id, or 'latest' (default)", "required": false }
            ]
        },
        {
            "name": "standup-summary",
            "description": "Draft a standup update from your recent meetings.",
            "arguments": []
        }
    ] })
}

pub fn prompt_get(db: &Db, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "meeting-context" => {
            let id = args
                .get("meeting_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("latest");
            let meta = tools::call(db, "get_meeting", &json!({ "meeting_id": id }))?;
            let transcript = tools::call(db, "get_transcript", &json!({ "meeting_id": id }))?;
            let text = format!(
                "Here is context from a meeting in Voco. Use it to answer my next question.\n\n\
                 ## Meeting\n{meta}\n\n## Transcript\n{transcript}"
            );
            Ok(prompt_message("Meeting context", text))
        }
        "standup-summary" => {
            let meetings = tools::call(db, "list_meetings", &json!({ "limit": 5 }))?;
            let text = format!(
                "Draft a concise standup update based on my recent Voco meetings below. \
                 Group by what's done, in progress, and blocked. If you need detail on a \
                 specific meeting, call get_transcript with its id.\n\n{meetings}"
            );
            Ok(prompt_message("Standup summary", text))
        }
        other => Err(format!("Unknown prompt '{other}'")),
    }
}

fn prompt_message(description: &str, text: String) -> Value {
    json!({
        "description": description,
        "messages": [
            { "role": "user", "content": { "type": "text", "text": text } }
        ]
    })
}
