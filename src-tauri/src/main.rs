// Prevents an extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod args;
mod buildroot;
mod commands;
mod config;
mod downloader;
mod extractor;
mod live;

use commands::AppState;
use std::sync::Mutex;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // Register this first so a second launch focuses the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let app_data_dir = app.path().app_data_dir().expect("no app data dir");
            let resource_dir = app
                .path()
                .resource_dir()
                .expect("no resource dir")
                .join("resources");

            std::fs::create_dir_all(&app_data_dir).ok();
            let mut cfg = config::load(&app_data_dir);
            let needs_config_import = !config::config_path(&app_data_dir).exists();
            let password_migrated = config::migrate_legacy_password(&mut cfg);
            let console_key_migrated = config::migrate_legacy_console_key(&mut cfg, &app_data_dir);
            config::migrate_legacy_support_files(&app_data_dir);
            if needs_config_import || password_migrated || console_key_migrated {
                if let Err(error) = config::save(&app_data_dir, &cfg) {
                    eprintln!("Could not persist imported launcher settings: {error}");
                }
            }
            let endpoints = live::load_endpoints(&resource_dir);

            app.manage(AppState {
                config: Mutex::new(cfg),
                app_data_dir: app_data_dir.clone(),
                resource_dir,
                endpoints: endpoints.clone(),
                install_phase: Mutex::new("idle".into()),
                abort: Mutex::new(None),
                game_pid: Mutex::new(None),
                game_message: Mutex::new("".into()),
                ui_lang: Mutex::new("en".into()),
                live_cache: live::LiveCache::default(),
                console_unlock_failures: Mutex::new(0),
                console_unlock_locked_until: Mutex::new(None),
            });

            live::spawn_health_poll(handle.clone(), endpoints.clone());
            live::spawn_news_poll(handle.clone(), endpoints.clone(), app_data_dir);
            live::spawn_discord_poll(handle.clone(), endpoints.clone());
            live::spawn_clients_poll(handle.clone(), endpoints.clone());
            live::spawn_events_stream(handle.clone(), endpoints.clone());

            let handle2 = handle.clone();
            let endpoints2 = endpoints.clone();
            tauri::async_runtime::spawn(async move {
                let state = handle2.state::<AppState>();
                let email = state.config.lock().unwrap().email.clone();
                live::fetch_profile(&handle2, &endpoints2, &email).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::complete_setup,
            commands::choose_folder,
            commands::check_folder,
            commands::open_folder,
            commands::save_account,
            commands::save_match_notifications,
            commands::get_state,
            commands::get_live_snapshot,
            commands::set_lang,
            commands::list_backgrounds,
            commands::list_live,
            commands::folder_info,
            commands::get_shop,
            commands::open_link,
            commands::open_official_site,
            commands::install_start,
            commands::install_pause,
            commands::install_cancel,
            commands::game_launch,
            commands::game_stop,
            commands::inject_module,
            commands::console_status,
            commands::console_unlock,
            commands::console_lock,
            commands::preflight,
            commands::check_launcher_update,
            commands::install_launcher_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
