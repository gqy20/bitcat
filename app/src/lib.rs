pub mod bubble;
pub mod commands;
pub mod gamepad;
pub mod joystick;
pub mod panel;
pub mod tray;
pub mod voice;

use bubble::SharedBubble;
use commands::SharedPet;
use voice::SharedVoice;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use ai_pad_core::action::{ActionConfig, ActionDef};
use ai_pad_core::bridge::handle_button_press;
use ai_pad_core::device::button_name;
use ai_pad_core::agent::PetAgent;
use ai_pad_core::hotkey;
use joystick::SdlGamepad;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{info, warn, error, debug, instrument};

/// 销毁旧 pet 窗口并用新尺寸重建，恢复到原位置。
///
/// Windows 上 decorations:false + transparent:true 的窗口调用 setSize() 会静默失败
/// (Tauri Issue #11975)，唯一可靠的方式是销毁后重建。
#[tauri::command]
async fn cmd_recreate_pet_window(
    app: tauri::AppHandle,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
) -> Result<(), String> {
    if let Some(old) = app.get_webview_window("pet") {
        old.close().map_err(|e| e.to_string())?;
    }

    let window = WebviewWindowBuilder::new(
        &app,
        "pet",
        WebviewUrl::App("pet.html".into()),
    )
    .title("8Bit Cat")
    .inner_size(width as f64, height as f64)
    .position(x as f64, y as f64)
    .decorations(false)
    .transparent(true)
    .background_color(tauri::webview::Color(0, 0, 0, 0))
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window.set_size(PhysicalSize::new(width, height));
    let _ = window.set_position(PhysicalPosition::new(x, y));

    Ok(())
}

/// 在光标位置弹出原生 Win32 右键菜单，返回被点击的菜单项 id
#[tauri::command]
async fn cmd_context_menu(app: tauri::AppHandle, collapsed: bool, always_on_top: bool) -> Result<String, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::Foundation::POINT;
    use std::ptr;

    unsafe {
        let menu = CreateMenu();
        if menu.is_null() {
            return Err("CreateMenu 失败".into());
        }

        let collapse_label = if collapsed { "展开" } else { "折叠" };
        let top_label = if always_on_top { "取消置顶" } else { "置顶" };

        AppendMenuW(menu, MF_STRING, 1, encode_wide(collapse_label).as_ptr());
        AppendMenuW(menu, MF_STRING, 2, encode_wide(top_label).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        AppendMenuW(menu, MF_STRING, 3, encode_wide("退出").as_ptr());

        // 获取光标位置
        let mut pt = std::mem::zeroed::<POINT>();
        GetCursorPos(&mut pt);

        let hwnd = app.get_webview_window("pet")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as windows_sys::Win32::Foundation::HWND)
            .unwrap_or(std::ptr::null_mut());
        let result = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            ptr::null(),
        );

        DestroyMenu(menu);

        match result {
            1 => Ok("collapse".into()),
            2 => Ok("top".into()),
            3 => Ok("exit".into()),
            _ => Ok("dismissed".into()),
        }
    }
}

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(SharedPet::new())
        .manage(SharedBubble::new())
        .manage(SharedVoice::new())
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
            bubble::cmd_consume_bubble_text,
            bubble::cmd_hide_bubble,
            voice::cmd_voice_update_text,
            voice::cmd_voice_get_text,
            cmd_recreate_pet_window,
            cmd_context_menu,
        ])
        .on_window_event(|window, event| {
            if window.label() == "panel" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            tray::create_tray(app.handle())?;

            if let Err(e) = voice::precreate_voice_window(app.handle()) {
                warn!(error = %e, "预创建 voice 窗口失败");
            }

            let app_handle = app.handle().clone();
            let hotkey_str = "CommandOrControl+Alt+Space";
            info!(hotkey = %hotkey_str, "准备注册全局热键");
            match hotkey_str.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                Ok(shortcut) => {
                    let shortcut_for_handler = shortcut.clone();
                    let handler_app = app_handle.clone();
                    let result = app.global_shortcut().on_shortcut(shortcut, move |_app, sc, evt| {
                        debug!(state = ? evt.state(), "热键回调触发");
                        if sc == &shortcut_for_handler && evt.state() == ShortcutState::Pressed {
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
                gamepad_loop(&handle);
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

/// 主游戏手柄循环 — 80ms tick，按键检测 → 状态机 → AI → 气泡/动作
#[instrument(skip(app))]
fn gamepad_loop(app: &tauri::AppHandle) {
    let sdl = match SdlGamepad::init() {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "SDL2 初始化失败");
            return;
        }
    };

    let pads = match SdlGamepad::list_gamepads(&sdl) {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "枚举手柄失败");
            return;
        }
    };

    if pads.is_empty() {
        warn!("未检测到游戏手柄，等待连接...");
        return;
    }

    for p in &pads {
        info!(index = p.index, name = %p.name, buttons = p.num_buttons, "设备 [{}] {} ({} 按钮)", p.index, p.name, p.num_buttons);
    }
    info!("ai-pad 启动");

    let mut gamepad = match SdlGamepad::open(&sdl, pads.first().map(|p| p.index).unwrap_or(0)) {
        Ok(g) => g,
        Err(e) => {
            error!(error = %e, "打开手柄失败");
            return;
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Tokio 运行时创建失败");
            return;
        }
    };

    let agent: std::sync::OnceLock<PetAgent> = std::sync::OnceLock::new();

    /// 懒加载：首次调用时才初始化 PetAgent（避免启动阻塞 2-5s）
    fn get_agent(agent: &std::sync::OnceLock<PetAgent>) -> Option<&PetAgent> {
        agent.get_or_init(|| match PetAgent::new() {
            Ok(a) => { info!("AI Agent 初始化成功 (8Bit Cat)"); a }
            Err(e) => { warn!(error = %e, "AI Agent 初始化失败"); panic!("PetAgent 初始化失败") }
        }).into()
    }

    let action_config = ActionConfig::load("actions.yml").ok();
    let ac = action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0);
    info!(action_count = ac, "已加载 {ac} 个动作绑定");

    let mut prev_buttons: u32 = 0;
    let mut prev_hat: Option<(i32, i32)> = None;
    let mut alt_tab = HeldModifier::new(0x12);  // VK_MENU
    let mut ctrl_tab = HeldModifier::new(0x11); // VK_CONTROL
    let mut held_voice = HeldCombo::new();

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
                    debug!(button_idx = idx, button_name = name, "按下 #{idx} {name}");

                    // 如果 Alt/Ctrl+Tab 按住中，按其他键时先释放
                    if (alt_tab.held && name != "L1") || (ctrl_tab.held && name != "L2") {
                        alt_tab.release();
                        ctrl_tab.release();
                    }

                    // Home 键 → 切换面板（独占处理）
                    if name == "Home" {
                        info!("→ 切换面板");
                        panel::toggle_panel(app);
                        continue;
                    }

                    // 面板可见 → 按键转发到面板
                    if panel_visible {
                        match name {
                            "A" => { info!("→ 面板确认"); let _ = app.emit("panel-confirm", ()); }
                            "B" => { info!("→ 面板关闭"); let _ = app.emit("panel-close", ()); }
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

                    if let (Some(msg), Some(ag)) = (&agent_msg, get_agent(&agent)) {
                        info!(msg = %msg, "→ AI: {msg}");
                        run_ai_chat(&rt, ag, app, msg, "");
                    }

                    // Actions: 按键名 → 动作绑定
                    if let Some(ref config) = action_config {
                        if let Some(action_def) = config.actions.get(name) {
                            info!(name = name, action_type = %action_def.action_type, "→ {} ({})", name, action_def.action_type);
                            execute_action(action_def, &config.defaults, &mut alt_tab, &mut ctrl_tab);
                        }
                    }
                }
            }
        }

        // Voice 按住检测
        let mut voice_just_released = false;
        let mut voice_just_pressed = false;
        if let Some(ref config) = action_config {
            let mut voice_bits: u32 = 0;
            for (name, action_def) in &config.actions {
                if action_def.action_type == "voice" {
                    if let Some(bit) = name_to_bit(name) {
                        voice_bits |= 1 << bit;
                    }
                }
            }
            let voice_active = (buttons & voice_bits) != 0;
            let (jp, jr) = held_voice.detect(voice_active);
            voice_just_pressed = jp;
            voice_just_released = jr;
        }

        // voice 按下
        if voice_just_pressed {
            match voice::open_voice_capture(app) {
                Ok(()) => info!("[voice] 录音条已显示并强制前台化"),
                Err(e) => warn!(error = %e, "[voice] 打开录音条失败"),
            }
            std::thread::sleep(std::time::Duration::from_millis(80));
            if let Some(ref config) = action_config {
                held_voice.press_keys(config);
            }
        }

        // voice 释放
        if voice_just_released {
            held_voice.release_keys();
            info!("[voice] 等待识别注入完成 (700ms)...");
            std::thread::sleep(std::time::Duration::from_millis(700));
            match voice::take_voice_text(app) {
                Ok(raw) => {
                    let text = raw.trim().to_string();
                    if text.is_empty() {
                        warn!("[voice] 虚拟输入框为空 (识别可能失败或焦点被抢走)");
                    } else {
                        info!(text = %text, len = text.chars().count(), "[voice] 识别全文: {text}");

                        if let Some(ag) = get_agent(&agent) {
                            run_ai_chat(&rt, ag, app, &text, "[voice]");
                        } else {
                            warn!("[voice] AI Agent 未初始化");
                        }
                    }
                }
                Err(e) => warn!(error = %e, "[voice] 读取虚拟输入框失败"),
            }
        }

        prev_buttons = buttons;

        // 方向键
        let hat = gamepad.read_hat(0);
        if panel_visible {
            if hat != prev_hat {
                if let Some((dx, dy)) = hat {
                    info!(dx = dx, dy = dy, "→ 面板导航");
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

/// 统一的 AI 流式对话执行：启动气泡 → 流式追加 chunk → 结束气泡 → 处理回复
fn run_ai_chat(
    rt: &tokio::runtime::Runtime,
    agent: &PetAgent,
    app: &tauri::AppHandle,
    msg: &str,
    log_prefix: &str,
) {
    let tag = if log_prefix.is_empty() { "" } else { " " };
    if let Err(e) = bubble::start_streaming_bubble(app) {
        warn!(error = %e, "{log_prefix}气泡启动错误");
        return;
    }
    let app_for_chunks = app.clone();
    let msg = msg.to_string();
    let prefix = log_prefix.to_string();
    let prefix_for_log = prefix.clone();
    let stream_result = rt.block_on(agent.chat_stream(&msg, move |chunk| {
        debug!(len = chunk.len(), "{prefix_for_log}{tag}AI chunk");
        let _ = bubble::append_bubble_chunk(&app_for_chunks, chunk);
    }));
    let _ = bubble::finalize_bubble(app);
    match stream_result {
        Ok(reply) => {
            if prefix.is_empty() {
                let preview: String = reply.chars().take(60).collect();
                info!(preview = %preview, "← AI: {preview}");
            } else {
                info!(chars = reply.chars().count(), reply = %reply, "{prefix} AI 回复全文 ({reply})");
            }
            let ai_events = gamepad::process_agent_response(&reply);
            for evt in &ai_events {
                let _ = app.emit("pet-event", evt);
            }
        }
        Err(e) => warn!(error = %e, "{prefix} AI 错误"),
    }
}

fn execute_action(
    action: &ActionDef,
    defaults: &ai_pad_core::action::Defaults,
    alt_tab: &mut HeldModifier,
    ctrl_tab: &mut HeldModifier,
) {
    match action.action_type.as_str() {
        "launch" => {
            let program = match &action.program {
                Some(p) => p.clone(),
                None => return,
            };
            let args = action.args.as_deref().unwrap_or("");
            let workdir = if action.workdir.is_empty() { ".".to_string() } else { action.workdir.clone() };

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
        "voice" => {}
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
                        warn!(error = %e, "热键触发失败");
                    }
                }
            }
        }
        other => {
            warn!(action_type = other, "未知动作类型");
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

/// 持续按住的多键组合(用于 voice 动作)
struct HeldCombo {
    vks: Vec<u16>,
    held: bool,
}

impl HeldCombo {
    fn new() -> Self { Self { vks: Vec::new(), held: false } }

    fn detect(&mut self, active: bool) -> (bool, bool) {
        match (active, self.held) {
            (true, false)  => { self.held = true;  (true, false) }
            (false, true) => { self.held = false; (false, true) }
            _              => (false, false),
        }
    }

    fn press_keys(&mut self, config: &ai_pad_core::action::ActionConfig) {
        let mut vks = Vec::new();
        for (_name, action_def) in &config.actions {
            if action_def.action_type == "voice" {
                if let Some(voice) = &action_def.voice {
                    let keys: Vec<&str> = voice.trigger.iter().map(|s| s.as_str()).collect();
                    vks.extend(hotkey::parse_keys(&keys));
                }
            }
        }
        self.vks = vks;
        for &vk in &self.vks {
            let _ = hotkey::key_down(vk);
        }
        info!(vk_count = self.vks.len(), "→ 输入法语音热键已按下");
    }

    fn release_keys(&mut self) {
        for &vk in self.vks.iter().rev() {
            let _ = hotkey::key_up(vk);
        }
        info!("→ 输入法语音热键已松开");
    }
}

fn name_to_bit(name: &str) -> Option<u32> {
    match name {
        "A"      => Some(0),
        "B"      => Some(1),
        "X"      => Some(3),
        "Y"      => Some(4),
        "L1"     => Some(6),
        "R1"     => Some(7),
        "L2"     => Some(8),
        "R2"     => Some(9),
        "Select" => Some(10),
        "Start"  => Some(11),
        "Home"   => Some(12),
        _ => None,
    }
}
