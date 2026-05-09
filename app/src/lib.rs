pub mod commands;
pub mod gamepad;
pub mod joystick;
pub mod panel;
pub mod tray;

use commands::SharedPet;
use tauri::{Emitter, Manager, WindowEvent};

use ai_pad_core::action::{ActionConfig, ActionDef};
use ai_pad_core::bridge::handle_button_press;
use ai_pad_core::device::button_name;
use ai_pad_core::agent::PetAgent;
use ai_pad_core::hotkey;
use joystick::SdlGamepad;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(SharedPet::new())
        .invoke_handler(tauri::generate_handler![
            commands::cmd_set_state,
            commands::cmd_walk_to,
            commands::cmd_show_bubble,
            commands::cmd_get_status,
            commands::cmd_tick,
            panel::cmd_show_panel,
            panel::cmd_hide_panel,
            panel::cmd_execute_panel_action,
            panel::cmd_panel_log,
        ])
        .on_window_event(|window, event| {
            // panel 失焦自动隐藏
            if window.label() == "panel" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            tray::create_tray(app.handle())?;

            // 注册全局热键 Ctrl+Alt+Space -> 切换 panel 显隐
            // 注：Alt+Space 被 Windows 系统占用为窗口菜单，无法用作全局热键
            let app_handle = app.handle().clone();
            let hotkey_str = "CommandOrControl+Alt+Space";
            log(&format!("准备注册全局热键: {hotkey_str}"));
            match hotkey_str.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                Ok(shortcut) => {
                    let shortcut_for_handler = shortcut.clone();
                    let handler_app = app_handle.clone();
                    let result = app.global_shortcut().on_shortcut(shortcut, move |_app, sc, evt| {
                        log(&format!("热键回调触发: state={:?}", evt.state()));
                        if sc == &shortcut_for_handler && evt.state() == ShortcutState::Pressed {
                            panel::toggle_panel(&handler_app);
                        }
                    });
                    match result {
                        Ok(_) => log(&format!("✓ 已注册 {hotkey_str} → 切换面板")),
                        Err(e) => log(&format!("✗ 注册 {hotkey_str} 失败: {e}")),
                    }
                }
                Err(e) => log(&format!("✗ 解析热键 {hotkey_str} 失败: {e}")),
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                gamepad_loop(&handle);
            });

            // 调试模式：自动弹出 panel + 模拟方向键事件，用于无手柄环境验证
            if std::env::var("AI_PAD_DEBUG").is_ok() {
                let dbg_app = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    log("[debug] 自动弹出 panel");
                    panel::toggle_panel(&dbg_app);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    log("[debug] 模拟 panel-nav (1, 0)");
                    let _ = dbg_app.emit("panel-nav", (1i32, 0i32));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    log("[debug] 模拟 panel-nav (1, 0)");
                    let _ = dbg_app.emit("panel-nav", (1i32, 0i32));
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    log("[debug] 模拟 panel-nav (0, -1)");
                    let _ = dbg_app.emit("panel-nav", (0i32, -1i32));
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn log(msg: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let ms = now.subsec_millis();
    eprintln!("[{h:02}:{m:02}:{s:02}.{ms:03}] {msg}");
}

fn gamepad_loop(app: &tauri::AppHandle) {
    let sdl = match SdlGamepad::init() {
        Ok(s) => s,
        Err(e) => {
            log(&format!("SDL2 初始化失败: {e}"));
            return;
        }
    };

    let pads = match SdlGamepad::list_gamepads(&sdl) {
        Ok(p) => p,
        Err(e) => {
            log(&format!("枚举手柄失败: {e}"));
            return;
        }
    };

    if pads.is_empty() {
        log("未检测到游戏手柄，等待连接...");
        return;
    }

    for p in &pads {
        log(&format!("设备 [{}] {} ({} 按钮)", p.index, p.name, p.num_buttons));
    }
    log("ai-pad 启动");

    let mut gamepad = match SdlGamepad::open(&sdl, pads.first().map(|p| p.index).unwrap_or(0)) {
        Ok(g) => g,
        Err(e) => {
            log(&format!("打开手柄失败: {e}"));
            return;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            log(&format!("Tokio 运行时创建失败: {e}"));
            return;
        }
    };

    let agent = match PetAgent::new() {
        Ok(a) => {
            log("AI Agent 初始化成功 (8Bit Cat)");
            Some(a)
        }
        Err(e) => {
            log(&format!("AI Agent 初始化失败: {e}"));
            None
        }
    };

    let action_config = ActionConfig::load("actions.yml").ok();
    log(&format!("已加载 {} 个动作绑定",
        action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0)));

    let mut prev_buttons: u32 = 0;
    let mut prev_hat: Option<(i32, i32)> = None;
    let mut alt_tab = HeldModifier::new(0x12);  // VK_MENU
    let mut ctrl_tab = HeldModifier::new(0x11); // VK_CONTROL

    loop {
        let panel_visible = app.get_webview_window("panel")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false);

        let buttons = gamepad.read_buttons();
        let new_presses = (buttons ^ prev_buttons) & buttons;

        if new_presses != 0 {
            for bit in 0..32 {
                if new_presses & (1 << bit) != 0 {
                    let idx = bit as u32;
                    let name = button_name(idx as usize).unwrap_or("?");
                    log(&format!("按下 #{idx} {name}"));

                    // 如果 Alt/Ctrl+Tab 按住中，按其他键时先释放
                    if (alt_tab.held && name != "L1") || (ctrl_tab.held && name != "L2") {
                        alt_tab.release();
                        ctrl_tab.release();
                    }

                    // Home 键 → 切换面板（独占处理，跳过 bridge/actions）
                    if name == "Home" {
                        log("  → 切换面板");
                        panel::toggle_panel(app);
                        continue;
                    }

                    // 面板可见 → 按键转发到面板，不再走 bridge/actions
                    if panel_visible {
                        match name {
                            "A" => {
                                log("  → 面板确认");
                                let _ = app.emit("panel-confirm", ());
                            }
                            "B" => {
                                log("  → 面板关闭");
                                let _ = app.emit("panel-close", ());
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Bridge: 特殊按键 → 宠物状态/AI
                    let (agent_msg, _pet_cmd) = handle_button_press(idx, "");

                    let events = gamepad::process_button(idx);
                    for evt in &events {
                        let _ = app.emit("pet-event", evt);
                    }

                    if let (Some(msg), Some(ag)) = (&agent_msg, &agent) {
                        log(&format!("  → AI: {msg}"));
                        match rt.block_on(ag.chat(msg)) {
                            Ok(reply) => {
                                let preview: String = reply.chars().take(60).collect();
                                log(&format!("  ← AI: {preview}"));
                                let ai_events = gamepad::process_agent_response(&reply);
                                for evt in &ai_events {
                                    let _ = app.emit("pet-event", evt);
                                }
                            }
                            Err(e) => {
                                log(&format!("  AI 错误: {e}"));
                            }
                        }
                    }

                    // Actions: 按键名 → 动作绑定（hotkey/launch/voice/script）
                    if let Some(ref config) = action_config {
                        if let Some(action_def) = config.actions.get(name) {
                            log(&format!("  → {} ({})", name, action_def.action_type));
                            execute_action(action_def, &config.defaults, &mut alt_tab, &mut ctrl_tab);
                        }
                    }
                }
            }
        }
        prev_buttons = buttons;

        // 方向键：面板可见 → 导航事件（边沿触发）；隐藏 → 滚动（持续触发）
        let hat = gamepad.read_hat(0);
        if panel_visible {
            if hat != prev_hat {
                if let Some((dx, dy)) = hat {
                    log(&format!("  → 面板导航 ({dx}, {dy})"));
                    let _ = app.emit("panel-nav", (dx, dy));
                }
            }
        } else if let Some((dx, dy)) = hat {
            alt_tab.release();
            ctrl_tab.release();
            let speed = 3;
            if dy > 0 { let _ = hotkey::send_scroll(120 * speed); }
            else if dy < 0 { let _ = hotkey::send_scroll(-120 * speed); }
            if dx > 0 { let _ = hotkey::send_scroll_h(120 * speed); }
            else if dx < 0 { let _ = hotkey::send_scroll_h(-120 * speed); }
        }
        prev_hat = hat;

        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

fn execute_action(
    action: &ActionDef,
    defaults: &ai_pad_core::action::Defaults,
    alt_tab: &mut HeldModifier,
    ctrl_tab: &mut HeldModifier,
) {
    // HeldModifier 定义在 gamepad_loop 内，这里用内联方式处理
    match action.action_type.as_str() {
        "launch" => {
            let program = match &action.program {
                Some(p) => p.clone(),
                None => return,
            };
            let args = action.args.as_deref().unwrap_or("");
            let workdir = if action.workdir.is_empty() { ".".into() } else { action.workdir.clone() };

            if action.terminal {
                let term = &defaults.terminal;
                let cmd = format!("cd {workdir}; {program} {args}");
                let full_args = format!(
                    "Start-Process {term} -ArgumentList '-NoExit','-Command','{cmd}' -WindowStyle Maximized"
                );
                let _ = std::process::Command::new("powershell")
                    .args(["-Command", &full_args])
                    .spawn();
            } else {
                let _ = std::process::Command::new(&program)
                    .args(args.split_whitespace())
                    .current_dir(&workdir)
                    .spawn();
            }
        }
        "voice" => {
            if let Some(voice) = &action.voice {
                let key_refs: Vec<&str> = voice.trigger.iter().map(|s| s.as_str()).collect();
                if let Err(e) = hotkey::trigger_hotkey(&key_refs, voice.delay) {
                    eprintln!("  热键触发失败: {e}");
                }
            }
        }
        "script" => {
            if let Some(cmd) = &action.command {
                let _ = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn();
            }
        }
        "hotkey" => {
            if let Some(trigger) = &action.trigger {
                let has_alt = trigger.iter().any(|k| k.to_lowercase() == "alt");
                let has_ctrl = trigger.iter().any(|k| k.to_lowercase() == "ctrl");
                let has_tab = trigger.iter().any(|k| k.to_lowercase() == "tab");

                if has_alt && has_tab {
                    alt_tab.press();
                } else if has_ctrl && has_tab {
                    ctrl_tab.press();
                } else {
                    let key_refs: Vec<&str> = trigger.iter().map(|s| s.as_str()).collect();
                    if let Err(e) = hotkey::trigger_hotkey(&key_refs, 0.02) {
                        eprintln!("  热键触发失败: {e}");
                    }
                }
            }
        }
        other => {
            eprintln!("  未知动作类型: {other}");
        }
    }
}

struct HeldModifier { vk: u16, held: bool }

impl HeldModifier {
    fn new(vk: u16) -> Self { Self { vk, held: false } }
    fn press(&mut self) {
        if !self.held {
            let _ = hotkey::key_down(self.vk);
            self.held = true;
        }
        let _ = hotkey::key_down(0x09);
        let _ = hotkey::key_up(0x09);
    }
    fn release(&mut self) {
        if self.held {
            let _ = hotkey::key_up(self.vk);
            self.held = false;
        }
    }
}
