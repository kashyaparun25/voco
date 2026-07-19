pub mod audio;
pub mod commands;
pub mod state;
pub mod storage;
pub mod stt;
pub mod services;
pub mod diarization;
pub mod providers;
pub mod llm;

use crate::state::AppState;
use crate::storage::Database;
use crate::stt::ModelManager;
use log::info;
use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_global_shortcut::ShortcutState;

/// Toggles dictation directly via the backend service (does NOT depend on the
/// main window being open) and shows/hides the pill accordingly.
fn trigger_dictation(app: &tauri::AppHandle) {
    use crate::services::dictation::DictationStatus;
    let state = app.state::<AppState>();
    let idle = matches!(state.dictation_service.get_status(), DictationStatus::Idle);
    if idle {
        info!("Hotkey: starting dictation");
        // Show the pill IMMEDIATELY for instant feedback, then start.
        let _ = crate::commands::window::show_pill(app);
        if let Err(e) = state.dictation_service.start(app.clone()) {
            log::warn!("Hotkey: failed to start dictation: {}", e);
            let _ = crate::commands::window::hide_pill(app);
        }
    } else {
        info!("Hotkey: stopping dictation");
        // Do NOT hide the pill here: transcription + paste are still running.
        // The pill switches to its processing spinner (dictation-status event)
        // and the dictation worker hides it AFTER the text is pasted — hiding
        // it now left users unsure whether the recording registered at all.
        let _ = state.dictation_service.stop();
    }
}

/// Focuses (creating visible + focused) the main window and emits a navigate event.
fn navigate_main(app: &tauri::AppHandle, route: &str) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    let _ = app.emit("navigate", route);
}

/// Initialise a file logger at ~/Library/Logs/Voco.log (GUI apps have no console),
/// plus stderr. Honors RUST_LOG, defaults to info.
fn init_logging() {
    use std::io::Write;
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    );
    if let Ok(home) = std::env::var("HOME") {
        let dir = std::path::PathBuf::from(&home).join("Library/Logs");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("Voco.log"))
        {
            builder.format(|buf, record| {
                writeln!(buf, "[{} {}] {}", chrono::Local::now().format("%H:%M:%S%.3f"), record.level(), record.args())
            });
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    let _ = builder.try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    info!("Voco starting up.");
    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_positioner::init())
        // Exclude the pill from window-state restore: it is positioned/sized
        // programmatically per-show, and restoring stale geometry made it appear
        // oversized or on the wrong monitor.
        .plugin(tauri_plugin_window_state::Builder::default().with_denylist(&["pill"]).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    // Only react on key press (not release) to avoid double-toggles.
                    if event.state() == ShortcutState::Pressed {
                        info!("Global shortcut triggered: {:?}", shortcut);
                        trigger_dictation(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Turn the pill window into a real nonactivating NSPanel. A plain
            // NSWindow of a regular app NEVER joins other Spaces or fullscreen
            // apps, whatever collectionBehavior it's given (verified live:
            // isOnActiveSpace stayed false over Chrome) — so the pill only ever
            // showed on Voco's own desktop. show_pill() applies the level /
            // collection behavior on every show.
            {
                use tauri_nspanel::WebviewWindowExt as _;
                match app.get_webview_window("pill").map(|w| w.to_panel()) {
                    Some(Ok(panel)) => {
                        // NSWindowStyleMaskNonactivatingPanel: interacting with
                        // the pill (stop button) must not steal focus from the
                        // app the user is dictating into.
                        panel.set_style_mask(1 << 7);
                        panel.set_hides_on_deactivate(false);
                        panel.set_becomes_key_only_if_needed(true);
                        info!("Pill window converted to nonactivating NSPanel.");
                    }
                    other => log::warn!("Pill NSPanel conversion failed: {:?}", other.map(|r| r.map(|_| ()))),
                }
            }

            // Initialize App Data directory and Database
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data dir");
            let db_path = app_data_dir.join("voco.db");
            
            info!("Initializing database at: {:?}", db_path);
            let db = Database::new(db_path).expect("Failed to open database");
            
            // Initialize Provider Registry and load defaults
            let provider_registry = crate::providers::ProviderRegistry::new(db.clone());
            provider_registry.initialize().expect("Failed to initialize provider registry");

            // Start automatic local server detection polling
            crate::providers::start_local_server_detection(app.handle().clone());
            
            // Initialize Model Manager
            let models_dir = app_data_dir.join("models");
            // Record the models dir so the embedded-LLM path can locate GGUF files.
            let _ = db.set_setting("models_dir", &models_dir.to_string_lossy());
            let model_manager = ModelManager::new(models_dir);
            // Re-register any user-added custom-URL models from previous sessions.
            crate::commands::models::load_custom_models(&db, &model_manager);

            // Recover any meeting audio left behind by an unclean shutdown (crash,
            // force-quit, power loss) and clear stale recording state. Runs before
            // the UI queries meeting state so nothing is lost or shown as a ghost.
            crate::services::meeting::recover_interrupted_recordings(
                &db,
                &app_data_dir.join("recordings"),
            );

            // Resolve the bundled MediaRemote helper (perl bridge + dylib) used to
            // pause/resume system media on macOS 15.4+ (where direct MediaRemote
            // access is entitlement-gated). No-op if the resources are missing.
            if let (Ok(pl), Ok(dylib)) = (
                app.path().resolve("resources/mediaremote-adapter.pl", tauri::path::BaseDirectory::Resource),
                app.path().resolve("resources/mediaremote-helper.dylib", tauri::path::BaseDirectory::Resource),
            ) {
                crate::services::media_control::init_helper(pl, dylib);
            }
            // Bundled dictation sound cues (resources/sounds/*.wav).
            if let Ok(sounds) = app
                .path()
                .resolve("resources/sounds", tauri::path::BaseDirectory::Resource)
            {
                crate::services::sound::init_sounds_dir(sounds);
            }

            // Set up state
            app.manage(AppState::new(db, model_manager));

            // Preload the STT engine in the background so the first hotkey
            // press starts recording instantly instead of waiting on model load.
            {
                let state: tauri::State<AppState> = app.state();
                let svc = state.dictation_service.clone();
                std::thread::spawn(move || svc.preload_engine());
            }

            // NOTE: window-vibrancy (frosted glass) was removed — applying
            // NSVisualEffectView to macOSPrivateApi windows is a suspected
            // contributor to a WindowServer crash and is purely cosmetic.

            // Set up tray icon and menu
            let quit_i = MenuItem::with_id(app, "quit", "Quit Voco", true, None::<&str>)?;
            let start_dictation_i = MenuItem::with_id(app, "start_dictation", "Start Dictation", true, None::<&str>)?;
            let start_meeting_i = MenuItem::with_id(app, "start_meeting", "Start Meeting", true, None::<&str>)?;
            let settings_i = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[&start_dictation_i, &start_meeting_i, &settings_i, &quit_i],
            )?;

            let mut tray_builder = TrayIconBuilder::new().menu(&menu);
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            
            let _tray = tray_builder
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "start_dictation" => {
                        info!("Tray: Start Dictation clicked");
                        trigger_dictation(app);
                    }
                    "start_meeting" => {
                        info!("Tray: Start Meeting clicked");
                        navigate_main(app, "meetings");
                    }
                    "settings" => {
                        info!("Tray: Settings clicked");
                        navigate_main(app, "settings");
                    }
                    _ => {}
                })
                .build(app)?;

            // Configurable dictation hotkey (default: Left Option). Bare-modifier
            // and double-tap triggers use a low-level (ListenOnly) event tap, which
            // requires Accessibility permission; combos and F-keys use the
            // global-shortcut plugin (no permission). apply_hotkey always also
            // registers ⌘⇧Space as a guaranteed fallback for bare modifiers.
            {
                let state: tauri::State<AppState> = app.state();
                let spec = state
                    .db
                    .get_setting("dictation_hotkey")
                    .ok()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "LeftOption".to_string());

                // Install the NSEvent hotkey monitors on the main thread (AppKit
                // requirement). Installed unconditionally — they are inert until
                // a bare-modifier spec is armed via apply_hotkey, so switching
                // hotkeys at runtime needs no reinstall.
                let handle_for_monitor = app.handle().clone();
                crate::services::hotkey::install_nsevent_monitors(move || {
                    trigger_dictation(&handle_for_monitor);
                });
                crate::services::hotkey::apply_hotkey(app.handle(), &spec);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio::list_audio_devices,
            commands::audio::set_audio_device,
            commands::dictation::start_dictation,
            commands::dictation::stop_dictation,
            commands::dictation::get_dictations,
            commands::dictation::get_dictation_audio_path,
            commands::dictation::delete_dictation,
            commands::meeting::start_meeting,
            commands::meeting::import_audio,
            commands::meeting::stop_meeting,
            commands::meeting::reprocess_meeting,
            commands::meeting::pause_meeting,
            commands::meeting::resume_meeting,
            commands::meeting::delete_meeting,
            commands::meeting::get_meetings,
            commands::meeting::get_meeting_transcript,
            commands::meeting::rename_speaker,
            commands::meeting::rename_meeting,
            commands::meeting::set_meeting_summary,
            commands::meeting::add_meeting_segment,
            commands::meeting::update_meeting_duration,
            commands::meeting::search_transcripts,
            commands::meeting::get_meeting_audio_path,
            commands::providers::get_providers,
            commands::providers::add_provider,
            commands::providers::update_provider,
            commands::providers::list_provider_models,
            commands::providers::delete_provider,
            commands::providers::set_active_provider,
            commands::providers::test_provider_connection,
            commands::providers::test_provider_config,
            commands::models::list_models,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::add_custom_model,
            commands::settings::get_settings,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::set_dictation_hotkey,
            commands::settings::check_accessibility_permission,
            commands::settings::request_accessibility_permission,
            commands::settings::check_input_monitoring_permission,
            commands::settings::request_input_monitoring_permission,
            commands::settings::check_screen_recording_permission,
            commands::settings::request_screen_recording_permission,
            commands::settings::check_microphone_permission,
            commands::settings::request_microphone_permission,
            commands::settings::read_app_logs,
            commands::settings::reveal_app_logs,
            commands::settings::clear_app_logs,
            commands::settings::list_sound_cue_styles,
            commands::settings::preview_sound_cue,
            commands::llm::summarize_meeting,
            commands::llm::summarize_meeting_streaming,
            commands::llm::regenerate_summary,
            commands::llm::ask_meeting_ai,
            commands::llm::suggest_meeting_title,
            commands::export::export_meeting,
            commands::export::export_meeting_to_path,
            commands::system::get_system_ram_mb,
            commands::system::recommend_models,
            commands::system::get_recordings_dir,
            commands::system::reveal_recordings_dir,
            commands::gcal::google_set_credentials,
            commands::gcal::google_status,
            commands::gcal::google_disconnect,
            commands::gcal::google_sign_in,
            commands::gcal::list_upcoming_meetings,
            commands::gcal::list_event_attendees_around,
            commands::gcal::list_event_titles_around,
            commands::stats::get_dictation_stats,
            commands::stats::set_typing_wpm,
            commands::stats::reset_dictation_stats,
            commands::window::show_pill_window,
            commands::window::hide_pill_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
