pub mod action_bus;
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

use gamepad::{SharedAgent, SharedChatCore, SharedPendingChat};
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
        .manage(SharedChatCore::new())
        .manage(SharedAgent::new())
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
            bubble::cmd_reposition_bubble,
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
            gamepad::cmd_enter_chat,
            gamepad::cmd_exit_chat,
            gamepad::cmd_pet_log,
            settings::cmd_settings_show,
            settings::cmd_settings_hide,
            settings::cmd_settings_close,
            settings::cmd_settings_log,
            settings::cmd_settings_load,
            settings::cmd_get_token_stats,
            settings::cmd_settings_save_ai,
            settings::cmd_settings_save_actions,
            settings::cmd_settings_save_prompts,
            settings::cmd_settings_save_user,
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
            if !env_loaded && dotenvy::dotenv().is_ok() {
                info!("已加载 .env (CWD)");
                env_loaded = true;
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

            // 桥接 core 的舞蹈播放事件 → 前端 pet 窗口的 play-dance 事件
            // AI 工具 execute_play_dance 往 channel 发请求，这里消费并 emit，
            // 同时维护 is_dancing 状态（供截图循环跳过本轮用）。
            let (dance_tx, mut dance_rx) =
                tokio::sync::mpsc::unbounded_channel::<ai_pad_core::dance::PlayDanceRequest>();
            if let Err(e) = ai_pad_core::dance::set_play_dance_sender(dance_tx) {
                warn!(error = %e, "注入 play_dance sender 失败");
            }
            let dance_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(req) = dance_rx.recv().await {
                    let name = req.name.clone();
                    let def = match ai_pad_core::dance::load_dance(&name) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!(error = %e, dance = %name, "加载舞蹈失败");
                            continue;
                        }
                    };

                    // 根据 loops / duration_ms 计算最终 loop_ 行为与硬上限
                    let one_round_ms = def.total_duration_ms() as u64;
                    let (loop_effective, total_ms): (bool, Option<u64>) = match req.loops {
                        Some(0) => (true, None),          // 无限循环
                        Some(n) if n >= 2 => (true, Some(one_round_ms.saturating_mul(n as u64))),
                        _ => (false, Some(one_round_ms)), // None / 1 = 单次
                    };
                    // duration_ms 若存在则取更小的硬上限
                    let max_ms: Option<u64> = match (total_ms, req.duration_ms) {
                        (Some(t), Some(d)) => Some(t.min(d as u64)),
                        (Some(t), None) => Some(t),
                        (None, Some(d)) => Some(d as u64),
                        (None, None) => None, // 仅在 loops=0 且未设 duration 时为真·无限
                    };

                    // 构造前端 payload：覆盖 loop_ + 附加 max_duration_ms
                    let mut payload = match serde_json::to_value(&def) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(error = %e, dance = %name, "DanceDef 序列化失败");
                            continue;
                        }
                    };
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert("loop_".into(), serde_json::json!(loop_effective));
                        if let Some(ms) = max_ms {
                            obj.insert("max_duration_ms".into(), serde_json::json!(ms));
                        }
                    }

                    ai_pad_core::dance::set_dancing(true);
                    if let Err(e) = bubble::hide_bubble_window(&dance_app) {
                        warn!(error = %e, dance = %name, "跳舞开始时隐藏 bubble 失败");
                    }
                    if let Err(e) = dance_app.emit("play-dance", &payload) {
                        warn!(error = %e, dance = %name, "emit play-dance 失败");
                        ai_pad_core::dance::set_dancing(false);
                        continue;
                    }
                    info!(
                        dance = %name,
                        loop_ = loop_effective,
                        max_ms = ?max_ms,
                        "[dance-bridge] 已 emit play-dance"
                    );

                    // 定时复位 IS_DANCING：若有硬上限则到时关闭；无限循环只靠下一次请求覆盖
                    if let Some(ms) = max_ms {
                        let guard_name = name.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                            ai_pad_core::dance::set_dancing(false);
                            debug!(dance = %guard_name, "[dance-bridge] 舞蹈时长到，IS_DANCING 复位");
                        });
                    }
                }
                warn!("[dance-bridge] channel 已关闭，消费任务退出");
            });

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
                                    action_bus::ActionBus::dispatch(
                                        &handler_app,
                                        action_bus::Action::TogglePanel,
                                        action_bus::ActionSource::Keyboard {
                                            shortcut: "CommandOrControl+Alt+Space".into(),
                                        },
                                    );
                                }
                            });
                    match result {
                        Ok(_) => info!(hotkey = %hotkey_str, "✓ 已注册 → 切换面板"),
                        Err(e) => warn!(error = %e, hotkey = %hotkey_str, "✗ 注册失败"),
                    }
                }
                Err(e) => warn!(error = %e, hotkey = %hotkey_str, "✗ 解析失败"),
            }

            // 批量注册 actions.yml 里 keyboard_shortcut 字段声明的全局热键。
            // 每条成功注册的热键都会通过 ActionBus 以 Keyboard source 分发对应 Action。
            // 老配置无此字段 → 跳过，完全向后兼容。
            match ai_pad_core::action::ActionConfig::load("config/actions.yml") {
                Ok(cfg) => {
                    let mut registered = 0usize;
                    for (name, def) in &cfg.actions {
                        let Some(sc_str) = def.keyboard_shortcut.clone() else {
                            continue;
                        };
                        let sc = match sc_str
                            .parse::<tauri_plugin_global_shortcut::Shortcut>()
                        {
                            Ok(s) => s,
                            Err(e) => {
                                warn!(error = %e, shortcut = %sc_str, button = %name, "解析键盘热键失败");
                                continue;
                            }
                        };
                        let Some(action_tpl) = action_bus::ActionBus::from_def(def) else {
                            warn!(button = %name, action_type = %def.action_type, "动作类型无法归一为 Action，跳过键盘热键注册");
                            continue;
                        };
                        let sc_expected = sc;
                        let lbl = sc_str.clone();
                        let btn = name.clone();
                        let handler_app = app_handle.clone();
                        let result = app.global_shortcut().on_shortcut(
                            sc,
                            move |_app, matched, evt| {
                                if matched == &sc_expected
                                    && evt.state() == ShortcutState::Pressed
                                {
                                    debug!(button = %btn, shortcut = %lbl, "键盘别名触发");
                                    action_bus::ActionBus::dispatch(
                                        &handler_app,
                                        action_tpl.clone(),
                                        action_bus::ActionSource::Keyboard {
                                            shortcut: lbl.clone(),
                                        },
                                    );
                                }
                            },
                        );
                        match result {
                            Ok(_) => {
                                registered += 1;
                                info!(button = %name, shortcut = %sc_str, "✓ 键盘别名已注册");
                            }
                            Err(e) => warn!(error = %e, button = %name, shortcut = %sc_str, "✗ 键盘别名注册失败"),
                        }
                    }
                    if registered > 0 {
                        info!(count = registered, "键盘别名共注册 {registered} 条");
                    }
                }
                Err(e) => warn!(error = %e, "加载 actions.yml 用于键盘别名注册失败"),
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad::gamepad_loop(&handle);
            });

            // 业务循环（独立于手柄）：消费 bubble 聊天输入 + 定时聚合长期记忆
            let chat_handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad::chat_loop(&chat_handle);
            });

            // 气泡跟随独立线程：脱离手柄循环，确保无手柄时也能实时跟随
            bubble::spawn_bubble_follower(app.handle().clone());

            let ss_handle = app.handle().clone();
            std::thread::spawn(move || {
                debug!("[screenshot] 截图线程已 spawn");
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
