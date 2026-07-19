//! Commands backing the Settings → Integrations panel: enable/disable the MCP
//! server, resolve the bundled `voco-mcp` sidecar, generate copy-paste client
//! configs, and run a live handshake to test the connection.

use crate::state::AppState;
use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::{AppHandle, Manager, State};

const MCP_ENABLED_KEY: &str = "mcp_enabled";

#[derive(Serialize)]
pub struct McpStatus {
    enabled: bool,
    sidecar_path: String,
    sidecar_exists: bool,
    db_path: String,
    meeting_count: i64,
    dictation_count: i64,
}

#[derive(Serialize)]
pub struct McpSetup {
    claude_code_cmd: String,
    cursor_json: String,
    generic_json: String,
    cursor_deeplink: String,
    setup_prompt: String,
}

#[derive(Serialize)]
pub struct McpTestResult {
    ok: bool,
    message: String,
}

/// The sidecar sits next to the app executable (Contents/MacOS/voco-mcp) in a
/// bundled build. In `tauri dev` it may instead be the workspace binaries copy.
fn sidecar_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("voco-mcp");
            if sibling.exists() {
                return sibling;
            }
            // Dev fallback: src-tauri/binaries/voco-mcp-<triple> relative to the
            // repo when running from target/debug.
            let triple = current_triple();
            let dev = dir
                .join("../../binaries")
                .join(format!("voco-mcp-{triple}"));
            if dev.exists() {
                if let Ok(c) = dev.canonicalize() {
                    return c;
                }
            }
            return sibling;
        }
    }
    PathBuf::from("voco-mcp")
}

fn current_triple() -> String {
    // Only aarch64-apple-darwin is shipped today; keep a sensible default.
    std::env::var("SIDECAR_TARGET").unwrap_or_else(|_| "aarch64-apple-darwin".to_string())
}

fn db_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("voco.db"))
        .unwrap_or_else(|_| PathBuf::from("voco.db"))
}

#[tauri::command]
pub fn mcp_get_status(app: AppHandle, state: State<'_, AppState>) -> Result<McpStatus, String> {
    let enabled = state
        .db
        .get_setting(MCP_ENABLED_KEY)
        .ok()
        .flatten()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let sp = sidecar_path();
    let count = |sql: &str| -> i64 {
        state.db.conn().query_row(sql, [], |r| r.get(0)).unwrap_or(0)
    };
    Ok(McpStatus {
        enabled,
        sidecar_exists: sp.exists(),
        sidecar_path: sp.to_string_lossy().to_string(),
        db_path: db_path(&app).to_string_lossy().to_string(),
        meeting_count: count("SELECT COUNT(*) FROM meetings"),
        dictation_count: count("SELECT COUNT(*) FROM dictations"),
    })
}

#[tauri::command]
pub fn mcp_set_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state
        .db
        .set_setting(MCP_ENABLED_KEY, if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mcp_get_setup() -> Result<McpSetup, String> {
    let path = sidecar_path().to_string_lossy().to_string();

    let claude_code_cmd = format!("claude mcp add voco -- {path}");

    let cursor_json = format!(
        "{{\n  \"mcpServers\": {{\n    \"voco\": {{\n      \"command\": \"{path}\"\n    }}\n  }}\n}}"
    );
    let generic_json = cursor_json.clone();

    // Cursor deep link: base64 of the server config object.
    let config_b64 = base64_encode(format!("{{\"command\":\"{path}\"}}").as_bytes());
    let cursor_deeplink =
        format!("cursor://anysphere.cursor-deeplink/mcp/install?name=voco&config={config_b64}");

    let setup_prompt = format!(
        "Add Voco (my local meeting-notes and dictation app) as an MCP server named `voco`. \
         The server binary is at `{path}` and uses stdio transport with no arguments and no \
         environment variables. Detect which agent you are and register it in your own config: \
         for Claude Code run `claude mcp add voco -- {path}`; for Cursor add it to \
         `~/.cursor/mcp.json` under `mcpServers.voco.command`; otherwise add the equivalent stdio \
         server entry to your MCP config. Then verify by calling the `get_status` tool and tell me \
         how many meetings and dictations you can see."
    );

    Ok(McpSetup {
        claude_code_cmd,
        cursor_json,
        generic_json,
        cursor_deeplink,
        setup_prompt,
    })
}

#[tauri::command]
pub fn mcp_test_connection(app: AppHandle) -> Result<McpTestResult, String> {
    let sp = sidecar_path();
    if !sp.exists() {
        return Ok(McpTestResult {
            ok: false,
            message: format!("Sidecar not found at {}. Rebuild or reinstall Voco.", sp.display()),
        });
    }

    let mut child = match Command::new(&sp)
        .env("VOCO_DB_PATH", db_path(&app))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(McpTestResult {
                ok: false,
                message: format!("Could not launch sidecar: {e}"),
            })
        }
    };

    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"voco-app","version":"1"}}}"#;
    let status = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_status","arguments":{}}}"#;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{init}");
        let _ = writeln!(stdin, "{status}");
        // Dropping stdin signals EOF so the sidecar finishes and exits.
    }

    let mut meeting_count = 0i64;
    let mut dictation_count = 0i64;
    let mut got_status = false;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if v.get("id").and_then(|i| i.as_i64()) == Some(2) {
                    if let Some(text) = v
                        .get("result")
                        .and_then(|r| r.get("content"))
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        if let Ok(status) = serde_json::from_str::<serde_json::Value>(text) {
                            meeting_count =
                                status.get("meeting_count").and_then(|n| n.as_i64()).unwrap_or(0);
                            dictation_count = status
                                .get("dictation_count")
                                .and_then(|n| n.as_i64())
                                .unwrap_or(0);
                            got_status = true;
                        }
                    }
                }
            }
        }
    }
    let _ = child.wait();

    if got_status {
        Ok(McpTestResult {
            ok: true,
            message: format!(
                "Connected. The server can see {meeting_count} meetings and {dictation_count} dictations."
            ),
        })
    } else {
        Ok(McpTestResult {
            ok: false,
            message: "Sidecar launched but did not respond as expected.".to_string(),
        })
    }
}

/// Minimal standard base64 (no padding omitted) for the Cursor deep link, so we
/// don't pull a crate into the app just for one string.
fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
