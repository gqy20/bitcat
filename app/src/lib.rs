//! app crate —— Tauri 2.0 应用壳，连接 core 逻辑与平台窗口/输入。
//!
//! 本模块是整个桌宠应用的运行入口。它负责创建 Tauri 窗口（pet / bubble /
//! panel / voice / settings）、注册 IPC 命令、加载 `.env` 与配置文件，
//! 并 spawn 手柄轮询、聊天循环、截图观察、气泡跟随等多个后台线程。
//! 核心业务逻辑全部在 `ai-pad-core` crate 中，app 只做胶水和平台集成。
//!
//! 与 core crate 的交互点：调用 `bridge`、`agent`、`memory`、`action`、
//! `dance`、`vision` 等模块的公共 API，把它们的输出通过 Tauri emit 推送到前端。
//! 与前端的交互点：通过 `app.emit()` 向 WebView2 窗口发送结构化事件，
//! 通过 `#[tauri::command]` 接收前端的 invoke 调用。
//!
//! ## unsafe 安全不变量
//!
//! 本文件自身不直接包含 unsafe 块，但它调用的子模块（`tray`、`screenshot` 等）
//! 使用了 Win32 API。这些 unsafe 的安全前提在各子模块的 `//!` 中单独说明。

pub mod action_bus;
pub mod agent_monitor;
pub mod agent_watch_window;
pub mod audio_reactive;
pub mod bubble;
pub mod claude_hooks;
pub mod codex_hooks;
pub mod commands;
pub mod game;
pub mod game_input;
pub mod gamepad;
pub mod joystick;
pub mod lifecycle;
pub mod observation_gate;
pub mod panel;
pub mod pet_event_bus;
pub mod pet_inbox;
pub mod remote_endpoint;
pub mod screenshot;
pub mod settings;
pub mod shutdown;
pub mod snap;
pub mod steam;
pub mod tray;
pub mod tts;
pub mod voice;

use gamepad::{SharedAgent, SharedChatCore, SharedPendingChat};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{debug, info, warn};

/// 应用主入口。构建 Tauri Builder，注册插件/状态/IPC 命令，在 setup 闭包中
/// 完成全部初始化（.env、托盘、热键、后台线程），最后启动事件循环。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // ── 共享状态注册 ──
        .manage(commands::SharedPet::default())
        .manage(commands::SharedWindowState::default())
        .manage(bubble::SharedBubble::new())
        .manage(audio_reactive::SharedAudioReactive::default())
        .manage(voice::SharedVoice::new())
        .manage(observation_gate::SharedObservationGate::default())
        .manage(screenshot::SharedScreenshotState::default())
        .manage(game::SharedGame::default())
        .manage(pet_event_bus::SharedPetEventBus::new())
        .manage(agent_monitor::SharedAgentMonitor::default())
        .manage(SharedPendingChat::new())
        .manage(SharedChatCore::new())
        .manage(SharedAgent::new())
        .on_menu_event(tray::handle_pet_context_menu_event)
        // ── IPC 命令注册 ──
        .invoke_handler(tauri::generate_handler![
            commands::cmd_set_state,
            commands::cmd_walk_to,
            commands::cmd_show_bubble,
            commands::cmd_get_status,
            commands::cmd_tick,
            commands::cmd_play_dance,
            commands::cmd_performance_finished,
            commands::cmd_get_window_state,
            audio_reactive::cmd_start_fake_music_dance,
            audio_reactive::cmd_start_wasapi_music_dance,
            audio_reactive::cmd_stop_music_dance,
            game::cmd_start_game,
            game::cmd_start_battle,
            game::cmd_start_memory,
            game::cmd_start_catch,
            game::cmd_start_game_with_def,
            game::cmd_get_current_game,
            game::cmd_game_end,
            game::cmd_game_cursor_position,
            game::cmd_game_set_input_capture,
            game::cmd_battle_pet_event,
            game::cmd_game_log,
            panel::cmd_show_panel,
            panel::cmd_hide_panel,
            panel::cmd_get_panel_actions,
            panel::cmd_execute_panel_action,
            panel::cmd_panel_log,
            bubble::cmd_consume_bubble_text,
            bubble::cmd_hide_bubble,
            bubble::cmd_reposition_bubble,
            voice::cmd_voice_update_text,
            voice::cmd_voice_get_text,
            snap::cmd_recreate_pet_window,
            snap::cmd_save_pet_position,
            snap::cmd_snap_pet,
            snap::cmd_snap_transform,
            snap::cmd_unsnap_transform,
            snap::cmd_get_snap_preview,
            tray::cmd_show_pet_context_menu,
            screenshot::cmd_screenshot_now,
            screenshot::cmd_get_hidden_screenshot_count,
            screenshot::cmd_clear_hidden_screenshot_count,
            screenshot::cmd_get_recent_screenshot_analyses,
            pet_inbox::cmd_show_pet_inbox,
            pet_inbox::cmd_hide_pet_inbox,
            gamepad::cmd_submit_chat,
            gamepad::cmd_open_chat,
            gamepad::cmd_enter_chat,
            gamepad::cmd_exit_chat,
            gamepad::cmd_pet_log,
            settings::cmd_settings_show,
            settings::cmd_settings_hide,
            settings::cmd_settings_close,
            settings::cmd_settings_log,
            agent_monitor::cmd_get_agent_sessions,
            agent_monitor::cmd_get_remote_install_cmd,
            agent_monitor::cmd_list_remote_devices,
            agent_monitor::cmd_dismiss_agent_session,
            agent_monitor::cmd_open_agent_workspace,
            agent_watch_window::cmd_agent_watch_hide,
            agent_watch_window::cmd_agent_watch_mark_user_placed,
            agent_watch_window::cmd_agent_watch_refresh,
            agent_watch_window::cmd_agent_watch_set_folded,
            agent_watch_window::cmd_agent_watch_port,
            agent_watch_window::cmd_agent_watch_log,
            claude_hooks::cmd_install_claude_code_hooks,
            claude_hooks::cmd_open_claude_settings,
            codex_hooks::cmd_install_codex_hooks,
            codex_hooks::cmd_open_codex_config,
            pet_event_bus::cmd_get_pet_event_log,
            settings::cmd_settings_load,
            settings::cmd_get_token_stats,
            settings::cmd_get_memory_review,
            settings::cmd_get_resource_usage,
            settings::cmd_delete_memory_entry,
            settings::cmd_settings_save_ai,
            settings::cmd_settings_save_actions,
            settings::cmd_settings_save_prompts,
            settings::cmd_settings_save_user,
            settings::cmd_settings_save_appearance,
            settings::cmd_settings_save_agent_watch,
            settings::cmd_settings_reset,
            settings::cmd_settings_apply,
        ])
        // ── 窗口事件处理 ──
        // panel 失焦自动隐藏；settings 关闭时拦截并隐藏而非销毁。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                crate::snap::save_visible_pet_position(window.app_handle());
            }
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
            let resource_dir = app
                .path()
                .resource_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|e| format!("<unavailable: {e}>"));
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.display().to_string()))
                .unwrap_or_else(|| "<unavailable>".to_string());
            info!(%resource_dir, %exe_dir, "tauri runtime paths");

            // ── .env 加载 ──
            // 只加载 exe 同目录 .env，避免 CWD/项目根 .env 抢占 AI 配置优先级。
            let env_path = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join(".env")));
            if let Some(env_path) = env_path {
                if env_path.exists() {
                    dotenvy::from_path_override(&env_path).ok();
                    info!(path = ?env_path, "已加载 exe 同目录 .env");
                } else {
                    warn!(path = ?env_path, "exe 同目录 .env 未找到");
                }
            } else {
                warn!("无法解析 exe 同目录 .env 路径");
            }

            // ── Steamworks 本地探针 ──
            // 动态加载 steam_api64.dll，只验证 AppID / Steam 客户端 / DLL 链路是否可用。
            // 深度功能（成就、云存档、Workshop）后续再接完整 wrapper。
            steam::init_probe();

            // ── 系统托盘 ──
            shutdown::install_ctrlc_handler(app.handle().clone());
            tray::create_tray(app.handle())?;

            // ── 表现桥接线程 ──
            // 消费 core 发来的 PlayDanceRequest，序列化 DanceDef 后以统一 performance 事件
            // 推送给 pet 窗口。会话状态由 core::performance 维护，供 bubble/screenshot 避让。
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

                    // 构造 timeline-dance payload：覆盖 loop_ + 附加 max_duration_ms
                    let mut dance_payload = match serde_json::to_value(&def) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(error = %e, dance = %name, "DanceDef 序列化失败");
                            continue;
                        }
                    };
                    if let Some(obj) = dance_payload.as_object_mut() {
                        obj.insert("loop_".into(), serde_json::json!(loop_effective));
                        if let Some(ms) = max_ms {
                            obj.insert("max_duration_ms".into(), serde_json::json!(ms));
                        }
                    }

                    let session = ai_pad_core::performance::start_performance(
                        ai_pad_core::performance::PerformanceKind::ChoreographedDance,
                    );
                    let payload = serde_json::json!({
                        "session_id": session.id,
                        "kind": "timeline-dance",
                        "dance": dance_payload,
                    });

                    if let Err(e) = bubble::hide_bubble_window(&dance_app) {
                        warn!(error = %e, dance = %name, "表现开始时隐藏 bubble 失败");
                    }
                    if let Err(e) = dance_app.emit("performance-start", &payload) {
                        warn!(error = %e, dance = %name, "emit performance-start 失败");
                        ai_pad_core::performance::stop_performance(session.id, "emit_failed");
                        continue;
                    }
                    ai_pad_core::performance::update_phase(
                        session.id,
                        ai_pad_core::performance::PerformancePhase::Active,
                    );
                    info!(
                        session_id = session.id,
                        dance = %name,
                        loop_ = loop_effective,
                        max_ms = ?max_ms,
                        "[performance-bridge] 已 emit performance-start"
                    );

                    // 定时兜底复位会话：若有硬上限则到时关闭；无限循环只靠前端 stop 或下一次请求覆盖
                    if let Some(ms) = max_ms {
                        let session_id = session.id;
                        let guard_name = name.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                            ai_pad_core::performance::stop_performance(session_id, "max_duration");
                            debug!(
                                session_id,
                                dance = %guard_name,
                                "[performance-bridge] 表现时长到，会话兜底复位"
                            );
                        });
                    }
                }
                warn!("[performance-bridge] channel 已关闭，消费任务退出");
            });

            // ── 预创建窗口 ──
            if let Err(e) = snap::precreate_pet_windows(app.handle()) {
                warn!(error = %e, "预创建 pet 窗口失败");
            }
            if let Err(e) = observation_gate::install_windows_observation_hooks(app.handle()) {
                warn!(error = %e, "安装屏幕观察门控失败");
            }

            if let Err(e) = voice::precreate_voice_window(app.handle()) {
                warn!(error = %e, "预创建 voice 窗口失败");
            }
            if let Err(e) = game::precreate_game_window(app.handle()) {
                warn!(error = %e, "预创建 game 窗口失败");
            }

            // ── 全局热键：面板切换 ──
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
                                    if crate::game::is_game_busy(&handler_app) {
                                        debug!(
                                            shortcut = "CommandOrControl+Alt+Space",
                                            "游戏运行中，跳过全局热键"
                                        );
                                        return;
                                    }
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

            // ── 全局热键：actions.yml 键盘别名 ──
            // 批量注册 actions.yml 里 keyboard_shortcut 字段声明的热键，
            // 通过 ActionBus 以 Keyboard source 分发对应 Action。
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
                                    if crate::game::is_game_busy(&handler_app) {
                                        debug!(
                                            button = %btn,
                                            shortcut = %lbl,
                                            "游戏运行中，跳过键盘别名"
                                        );
                                        return;
                                    }
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

            // ── 后台线程 ──
            agent_monitor::spawn_agent_monitor(app.handle().clone());
            agent_monitor::spawn_agent_view_server(app.handle().clone());

            // 手柄轮询线程：SDL2 80ms tick，处理按键→命令→AI 对话。
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad::gamepad_loop(&handle);
            });

            // 聊天业务线程：消费 bubble 聊天输入 + 定时聚合长期记忆。
            let chat_handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad::chat_loop(&chat_handle);
            });

            // 气泡跟随线程：脱离手柄循环，确保无手柄时也能实时跟随。
            bubble::spawn_bubble_follower(app.handle().clone());
            agent_watch_window::spawn_agent_watch_follower(app.handle().clone());

            // 截图观察线程：定时 BitBlt 截屏 + Vision API 分析。
            let ss_handle = app.handle().clone();
            std::thread::spawn(move || {
                debug!("[screenshot] 截图线程已 spawn");
                screenshot::screenshot_loop(&ss_handle);
            });

            // ── Debug 辅助 ──
            // AI_PAD_DEBUG=1 时自动弹出 panel 并模拟导航操作，用于开发调试。
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
