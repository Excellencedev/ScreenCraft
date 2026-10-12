#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ax;
mod capture;
mod commands;
mod compress;
mod inputlog;
mod meter;
mod state;
mod windows;

use commands::permissions::all_required_granted;
use state::{load_settings, AppState};
use std::fs;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn main() {
    let toggle = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Digit2);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &toggle && event.state() == ShortcutState::Pressed {
                        let _ = app.emit("toggle-record", ());
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                windows::on_window_destroyed(window.app_handle(), window.label());
            }
        })
        .manage(AppState {
            capture: capture::Capture::spawn(),
            meter: Default::default(),
            recording_dir: Mutex::new(None),
            camera: Mutex::new(None),
            export: Mutex::new(None),
            inputs: Mutex::new(Default::default()),
            settings: Mutex::new(load_settings()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::sources::list_sources,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::camera_begin,
            commands::recording::camera_chunk,
            commands::projects::load_project,
            commands::projects::save_project,
            commands::projects::compute_render_track,
            commands::projects::auto_zoom,
            commands::projects::recent_projects,
            commands::projects::projects_dir,
            commands::export::export_begin,
            commands::export::export_write,
            commands::export::export_end,
            commands::files::reveal_in_finder,
            commands::files::copy_file,
            commands::files::get_settings,
            commands::files::set_projects_dir,
            commands::compress::probe_video,
            commands::compress::compress_video,
            commands::compress::cancel_compress,
            commands::compress::estimate_bitrate,
            commands::permissions::permission_status,
            commands::permissions::request_screen_access,
            commands::permissions::request_input_monitoring,
            commands::permissions::open_privacy_pane,
            commands::permissions::restart_app,
            commands::devices::list_audio_inputs,
            commands::devices::list_cameras,
            commands::devices::meter_start,
            commands::devices::meter_stop,
            windows::show_recorder,
            windows::show_permissions,
            windows::show_compressor,
            windows::open_editor,
            windows::open_area_picker,
            windows::show_area_frame,
            windows::hide_area_frame,
        ])
        .setup(move |app| {
            let _ = app.global_shortcut().register(toggle);
            let new_rec = MenuItem::with_id(app, "new", "New Recording", true, Some("CmdOrCtrl+Shift+2"))?;
            let compress = MenuItem::with_id(app, "compress", "Compress Video…", true, None::<&str>)?;
            let perms = MenuItem::with_id(app, "perms", "Permissions…", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop Recording", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit ScreenCraft", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&new_rec, &compress, &perms, &stop, &quit])?;
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().cloned().ok_or("no icon")?)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "new" => {
                        let _ = windows::show_recorder_window(app);
                    }
                    "perms" => {
                        let _ = windows::show_permissions_window(app);
                    }
                    "compress" => {
                        let _ = windows::show_compressor(app.clone(), None);
                    }
                    "stop" => {
                        let _ = app.emit("stop-record", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            let state = app.state::<AppState>();
            fs::create_dir_all(state.projects_root()).ok();
            if all_required_granted() {
                windows::show_recorder_window(app.handle())?;
            } else {
                windows::show_permissions_window(app.handle())?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ScreenCraft");
}
