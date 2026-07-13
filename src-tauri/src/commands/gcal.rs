//! Google Calendar integration via desktop OAuth (loopback + client-secret flow).
//!
//! The developer/user supplies a Google Cloud "Desktop app" OAuth Client ID +
//! secret (Calendar API enabled). We run the standard installed-app flow:
//! open the consent page in the browser, catch the redirect on a localhost
//! loopback server, exchange the code for tokens, and store them. Access tokens
//! auto-refresh via the refresh token.

use crate::state::AppState;
use crate::storage::Database;
use serde::Serialize;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::State;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly email";

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Percent-encode a query-string value.
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn get(db: &Database, key: &str) -> Option<String> {
    db.get_setting(key).ok().flatten().filter(|s| !s.trim().is_empty())
}

#[derive(Serialize)]
pub struct GoogleStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub has_credentials: bool,
}

#[tauri::command]
pub fn google_set_credentials(state: State<'_, AppState>, client_id: String, client_secret: String) -> Result<(), String> {
    state.db.set_setting("google_client_id", client_id.trim()).map_err(|e| e.to_string())?;
    state.db.set_setting("google_client_secret", client_secret.trim()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn google_status(state: State<'_, AppState>) -> GoogleStatus {
    GoogleStatus {
        connected: get(&state.db, "google_refresh_token").is_some(),
        email: get(&state.db, "google_email"),
        has_credentials: get(&state.db, "google_client_id").is_some() && get(&state.db, "google_client_secret").is_some(),
    }
}

#[tauri::command]
pub fn google_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    for k in ["google_refresh_token", "google_access_token", "google_token_expiry", "google_email"] {
        let _ = state.db.set_setting(k, "");
    }
    Ok(())
}

/// Run the OAuth flow: returns the connected account email.
#[tauri::command]
pub async fn google_sign_in(state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db.clone();
    let client_id = get(&db, "google_client_id").ok_or("Set your Google OAuth Client ID first.")?;
    let client_secret = get(&db, "google_client_secret").ok_or("Set your Google OAuth Client Secret first.")?;

    // Loopback server on a random port (Google allows any 127.0.0.1 port for Desktop clients).
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Failed to open loopback: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let state_tok = uuid::Uuid::new_v4().to_string();

    let auth = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
        enc(&client_id), enc(&redirect), enc(SCOPE), enc(&state_tok)
    );
    std::process::Command::new("open").arg(&auth).spawn().map_err(|e| format!("Failed to open browser: {e}"))?;

    let code = wait_for_code(listener, &state_tok).await?;

    // Exchange code for tokens.
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| format!("Token exchange failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Token exchange rejected: {}", resp.text().await.unwrap_or_default()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let access = json["access_token"].as_str().ok_or("No access_token in response")?.to_string();
    let refresh = json["refresh_token"].as_str().unwrap_or("").to_string();
    let expires_in = json["expires_in"].as_i64().unwrap_or(3600);
    if refresh.is_empty() {
        return Err("Google did not return a refresh token (try disconnecting and reconnecting).".to_string());
    }

    let _ = db.set_setting("google_access_token", &access);
    let _ = db.set_setting("google_refresh_token", &refresh);
    let _ = db.set_setting("google_token_expiry", &(unix_now() + expires_in).to_string());

    // Fetch the account email.
    let email = match client.get("https://www.googleapis.com/oauth2/v2/userinfo").bearer_auth(&access).send().await {
        Ok(resp) if resp.status().is_success() => {
            let j: serde_json::Value = resp.json().await.unwrap_or_default();
            j["email"].as_str().unwrap_or("").to_string()
        }
        _ => String::new(),
    };
    if !email.is_empty() {
        let _ = db.set_setting("google_email", &email);
    }
    Ok(if email.is_empty() { "Connected".to_string() } else { email })
}

/// Wait for Google to redirect to our loopback with the auth code.
async fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).ok();
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let first = req.lines().next().unwrap_or("");
                let path = first.split_whitespace().nth(1).unwrap_or("");
                let query = path.split('?').nth(1).unwrap_or("");
                let mut code: Option<String> = None;
                let mut st: Option<String> = None;
                let mut err: Option<String> = None;
                for kv in query.split('&') {
                    let mut it = kv.splitn(2, '=');
                    let k = it.next().unwrap_or("");
                    let v = it.next().unwrap_or("");
                    match k {
                        "code" => code = Some(urldecode(v)),
                        "state" => st = Some(urldecode(v)),
                        "error" => err = Some(urldecode(v)),
                        _ => {}
                    }
                }
                let msg = if err.is_some() { "Sign-in was cancelled." } else { "Voco is connected \u{2713} — you can close this window." };
                let body = format!("<html><body style='font-family:-apple-system,sans-serif;padding:48px;text-align:center'><h2>{msg}</h2></body></html>");
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes());

                if let Some(e) = err {
                    return Err(format!("Google returned error: {e}"));
                }
                if st.as_deref() == Some(expected_state) {
                    if let Some(c) = code {
                        return Ok(c);
                    }
                }
                // Otherwise (e.g. a /favicon.ico hit) keep waiting.
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("Timed out waiting for Google sign-in.".to_string());
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

async fn valid_access_token(db: &Database) -> Result<String, String> {
    let expiry = get(db, "google_token_expiry").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    if let Some(at) = get(db, "google_access_token") {
        if unix_now() < expiry - 60 {
            return Ok(at);
        }
    }
    // Refresh.
    let refresh = get(db, "google_refresh_token").ok_or("Google Calendar not connected.")?;
    let client_id = get(db, "google_client_id").ok_or("Missing client id")?;
    let client_secret = get(db, "google_client_secret").ok_or("Missing client secret")?;
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("refresh_token", refresh.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Token refresh rejected: {}", resp.text().await.unwrap_or_default()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let access = json["access_token"].as_str().ok_or("No access_token")?.to_string();
    let expires_in = json["expires_in"].as_i64().unwrap_or(3600);
    let _ = db.set_setting("google_access_token", &access);
    let _ = db.set_setting("google_token_expiry", &(unix_now() + expires_in).to_string());
    Ok(access)
}

#[derive(Serialize)]
pub struct UpcomingMeeting {
    pub id: String,
    pub title: String,
    pub start: String, // RFC3339
    pub end: String,
    pub attendees: Vec<String>,
}

#[tauri::command]
pub async fn list_upcoming_meetings(state: State<'_, AppState>) -> Result<Vec<UpcomingMeeting>, String> {
    let db = state.db.clone();
    let access = valid_access_token(&db).await?;

    let now = chrono::Utc::now();
    let max = now + chrono::Duration::hours(12);
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime&maxResults=25",
        enc(&now.to_rfc3339()), enc(&max.to_rfc3339())
    );
    let client = reqwest::Client::new();
    let resp = client.get(&url).bearer_auth(&access).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Calendar fetch failed: {}", resp.text().await.unwrap_or_default()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(items) = json["items"].as_array() {
        for it in items {
            let start = it["start"]["dateTime"].as_str().or_else(|| it["start"]["date"].as_str()).unwrap_or("").to_string();
            let end = it["end"]["dateTime"].as_str().or_else(|| it["end"]["date"].as_str()).unwrap_or("").to_string();
            let attendees = it["attendees"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter(|a| !a["self"].as_bool().unwrap_or(false)) // skip yourself
                        .filter_map(|a| a["displayName"].as_str().or_else(|| a["email"].as_str()))
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            out.push(UpcomingMeeting {
                id: it["id"].as_str().unwrap_or("").to_string(),
                title: it["summary"].as_str().unwrap_or("(no title)").to_string(),
                start,
                end,
                attendees,
            });
        }
    }
    Ok(out)
}
