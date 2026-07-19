//! voco-mcp — a read-only Model Context Protocol server for Voco.
//!
//! MCP's stdio transport is newline-delimited JSON-RPC 2.0: one message per
//! line on stdin, one response per line on stdout. This binary is spawned by
//! coding agents (Claude Code, Cursor, …) and exposes the user's meetings,
//! transcripts, and dictation history as tools/resources/prompts. It never
//! writes to the database.

mod content;
mod db;
mod tools;

use db::Db;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

struct ServerState {
    db: Option<Db>,
    db_path: PathBuf,
    client_name: Option<String>,
}

fn main() {
    let db_path = resolve_db_path();
    let db = Db::open_readonly(&db_path).ok();
    let mut state = ServerState {
        db,
        db_path,
        client_name: None,
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin closed
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let responses: Vec<Value> = batch
                    .into_iter()
                    .filter_map(|m| handle(&mut state, &m))
                    .collect();
                if responses.is_empty() {
                    None
                } else {
                    Some(Value::Array(responses))
                }
            }
            Ok(msg) => handle(&mut state, &msg),
            Err(_) => Some(json!({
                "jsonrpc": "2.0", "id": Value::Null,
                "error": { "code": -32700, "message": "Parse error" }
            })),
        };
        if let Some(resp) = response {
            if serde_json::to_writer(&mut out, &resp).is_err() {
                break;
            }
            if out.write_all(b"\n").is_err() || out.flush().is_err() {
                break;
            }
        }
    }
}

/// Handle one JSON-RPC message. Returns `Some(response)` for requests, `None`
/// for notifications (no `id`).
fn handle(state: &mut ServerState, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    // Notifications carry no id and expect no response.
    if id.is_none() {
        if method == "notifications/initialized" {
            // nothing to do
        }
        return None;
    }
    let id = id.unwrap();

    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            state.client_name = params
                .get("clientInfo")
                .and_then(|c| c.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
            Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                },
                "serverInfo": { "name": "voco", "version": SERVER_VERSION },
                "instructions": "Voco exposes your on-device meeting transcripts, summaries, notes, and dictation history. Call get_status first to confirm the connection."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools::tool_list()),
        "tools/call" => Some(handle_tool_call(state, &params)).unwrap(),
        "resources/list" => Ok(content::resource_list()),
        "resources/templates/list" => Ok(content::resource_templates_list()),
        "resources/read" => handle_resource_read(state, &params),
        "prompts/list" => Ok(content::prompt_list()),
        "prompts/get" => handle_prompt_get(state, &params),
        other => Err((-32601, format!("Method not found: {other}"))),
    };

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

fn handle_tool_call(state: &mut ServerState, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or((-32602, "Missing tool name".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    log_call(state, name, &args);

    // get_status is always available — it's how the UI/agent diagnoses setup.
    if name == "get_status" {
        return Ok(tool_text(get_status(state), false));
    }

    match ready_db(state) {
        Ok(db) => match tools::call(db, name, &args) {
            Ok(text) => Ok(tool_text(text, false)),
            Err(e) => Ok(tool_text(e, true)),
        },
        Err(e) => Ok(tool_text(e, true)),
    }
}

fn handle_resource_read(state: &ServerState, params: &Value) -> Result<Value, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(|u| u.as_str())
        .ok_or((-32602, "Missing resource uri".to_string()))?;
    let db = ready_db(state).map_err(|e| (-32002, e))?;
    content::resource_read(db, uri).map_err(|e| (-32002, e))
}

fn handle_prompt_get(state: &ServerState, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or((-32602, "Missing prompt name".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let db = ready_db(state).map_err(|e| (-32002, e))?;
    content::prompt_get(db, name, &args).map_err(|e| (-32602, e))
}

/// A usable DB: present, recognized schema, and MCP enabled in Voco settings.
fn ready_db(state: &ServerState) -> Result<&Db, String> {
    let db = state.db.as_ref().ok_or_else(|| {
        format!(
            "Voco database not found at {}. Launch Voco at least once, then retry.",
            state.db_path.display()
        )
    })?;
    if !db.schema_ok() {
        return Err("Voco database schema not recognized — update Voco to the latest version.".into());
    }
    if !db.mcp_enabled() {
        return Err(
            "The Voco MCP server is disabled. Turn it on in Voco → Settings → Integrations.".into(),
        );
    }
    Ok(db)
}

fn get_status(state: &ServerState) -> String {
    let (enabled, schema_ok, meetings, dictations) = match &state.db {
        Some(db) => (
            db.mcp_enabled(),
            db.schema_ok(),
            db.count("meetings"),
            db.count("dictations"),
        ),
        None => (false, false, 0, 0),
    };
    let v = json!({
        "server": "voco-mcp",
        "version": SERVER_VERSION,
        "protocol_version": PROTOCOL_VERSION,
        "enabled": enabled,
        "db_path": state.db_path.display().to_string(),
        "db_found": state.db.is_some(),
        "schema_ok": schema_ok,
        "meeting_count": meetings,
        "dictation_count": dictations,
        "hint": if !state.db.is_some() {
            "Database missing — launch Voco once."
        } else if !enabled {
            "Enable the server in Voco → Settings → Integrations."
        } else {
            "Ready."
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
}

fn tool_text(text: String, is_error: bool) -> Value {
    let mut v = json!({ "content": [ { "type": "text", "text": text } ] });
    if is_error {
        v["isError"] = Value::Bool(true);
    }
    v
}

/// Default macOS location, overridable via VOCO_DB_PATH (used in dev/tests).
fn resolve_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("VOCO_DB_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library/Application Support/com.kashy.voco/voco.db")
}

/// Best-effort append to ~/Library/Logs/Voco-MCP.log so users can see what their
/// agents read. Never logs transcript/note bodies — only the tool and its args.
fn log_call(state: &ServerState, tool: &str, args: &Value) {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let path = PathBuf::from(home).join("Library/Logs/Voco-MCP.log");
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let client = state.client_name.as_deref().unwrap_or("unknown");
    let mut args_str = args.to_string();
    if args_str.len() > 200 {
        args_str.truncate(200);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{ms}\t{client}\t{tool}\t{args_str}");
    }
}
