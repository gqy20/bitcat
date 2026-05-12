pub mod bubble;
pub mod commands;
pub mod gamepad;
pub mod joystick;
pub mod panel;
pub mod screenshot;
pub mod settings;
pub mod snap;
pub mod tray;
pub mod tts;
pub mod voice;

use gamepad::SharedPendingChat;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{debug, info, warn};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::SharedPet::default())
        .manage(commands::SharedWindowState::default())
        .manage(bubble::SharedBubble::new())
        .manage(voice::SharedVoice::new())
        .manage(screenshot::SharedScreenshotState::default())
        .manage(SharedPendingChat::new())
        .invoke_handler(tauri::generate_handler![
            commands::cmd_set_state,
            commands::cmd_walk_to,
            commands::cmd_show_bubble,
            commands::cmd_get_status,
            commands::cmd_tick,
            commands::cmd_play_dance,
            commands::cmd_get_window_state,
            panel::cmd_show_panel,
            panel::cmd_hide_panel,
            panel::cmd_execute_panel_action,
            panel::cmd_panel_log,
            bubble::cmd_consume_bubble_text,
            bubble::cmd_hide_bubble,
            voice::cmd_voice_update_text,
            voice::cmd_voice_get_text,
            snap::cmd_recreate_pet_window,
            snap::cmd_snap_pet,
            snap::cmd_snap_transform,
            snap::cmd_unsnap_transform,
            snap::cmd_get_snap_preview,
            screenshot::cmd_screenshot_now,
            gamepad::cmd_submit_chat,
            gamepad::cmd_open_chat,
            gamepad::cmd_exit_chat,
            gamepad::cmd_pet_log,
            settings::cmd_settings_show,
            settings::cmd_settings_hide,
            settings::cmd_settings_close,
            settings::cmd_settings_log,
            settings::cmd_settings_load,
            settings::cmd_settings_save_ai,
            settings::cmd_settings_save_actions,
            settings::cmd_settings_save_prompts,
            settings::cmd_settings_save_appearance,
            settings::cmd_settings_reset,
            settings::cmd_settings_apply,
        ])
        .on_window_event(|window, event| {
            if window.label() == "panel" {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // 加载 .env
            let mut env_loaded = false;
            if let Some(exe_dir) = app
                .path()
                .resource_dir()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            {
                let env_path = exe_dir.join(".env");
                if env_path.exists() {
                    dotenvy::from_path(&env_path).ok();
                    info!(path = ?env_path, "已加载 .env");
                    env_loaded = true;
                }
            }
            if !env_loaded {
                if dotenvy::dotenv().is_ok() {
                    info!("已加载 .env (CWD)");
                    env_loaded = true;
                }
            }
            if !env_loaded {
                if let Some(exe_dir) = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                {
                    let fallback = exe_dir.join(".env");
                    if fallback.exists() {
                        dotenvy::from_path(&fallback).ok();
                        info!(path = ?fallback, "已加载 .env (项目根目录)");
                        env_loaded = true;
                    }
                }
            }
            if !env_loaded {
                warn!(".env 未找到，将使用 ~/.claude/settings.json 或默认配置");
            }

            tray::create_tray(app.handle())?;

            if let Err(e) = snap::precreate_pet_windows(app.handle()) {
                warn!(error = %e, "预创建 pet 窗口失败");
            }

            if let Err(e) = voice::precreate_voice_window(app.handle()) {
                warn!(error = %e, "预创建 voice 窗口失败");
            }

            let app_handle = app.handle().clone();
            let hotkey_str = "CommandOrControl+Alt+Space";
            info!(hotkey = %hotkey_str, "准备注册全局热键");
            match hotkey_str.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                Ok(shortcut) => {
                    let shortcut_for_handler = shortcut;
                    let handler_app = app_handle.clone();
                    let result =
                        app.global_shortcut()
                            .on_shortcut(shortcut, move |_app, sc, evt| {
                                debug!(state = ? evt.state(), "热键回调触发");
                                if sc == &shortcut_for_handler
                                    && evt.state() == ShortcutState::Pressed
                                {
                                    panel::toggle_panel(&handler_app);
                                }
                            });
                    match result {
                        Ok(_) => info!(hotkey = %hotkey_str, "✓ 已注册 → 切换面板"),
                        Err(e) => warn!(error = %e, hotkey = %hotkey_str, "✗ 注册失败"),
                    }
                }
                Err(e) => warn!(error = %e, hotkey = %hotkey_str, "✗ 解析失败"),
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad::gamepad_loop(&handle);
            });

            // 气泡跟随独立线程：脱离手柄循环，确保无手柄时也能实时跟随
            bubble::spawn_bubble_follower(app.handle().clone());

            let ss_handle = app.handle().clone();
            std::thread::spawn(move || {
                eprintln!("[SS-DBG] 截图线程已 spawn");
                screenshot::screenshot_loop(&ss_handle);
            });

            if std::env::var("AI_PAD_DEBUG").is_ok() {
                let dbg_app = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    info!("[debug] 自动弹出 panel");
                    panel::toggle_panel(&dbg_app);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    info!("[debug] 模拟 panel-nav (1, 0)");
                    let _ = dbg_app.emit("panel-nav", (1i32, 0i32));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    info!("[debug] 模拟 panel-nav (1, 0)");
                    let _ = dbg_app.emit("panel-nav", (1i32, 0i32));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    info!("[debug] 模拟 panel-nav (0, -1)");
                    let _ = dbg_app.emit("panel-nav", (0i32, -1i32));
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
