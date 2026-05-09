#![windows_subsystem = "windows"]

use windows_sys::Win32::System::Console::AllocConsole;
use windows_sys::Win32::System::Threading::CreateMutexW;

use ai_pad_core::bridge::{handle_button_press, resolve_agent_response, PetCommand};
use ai_pad_core::config::ButtonConfig;
use ai_pad_core::action::ActionConfig;
use ai_pad_core::device::button_name;
use ai_pad_core::agent::PetAgent;
use ai_pad_ctl::joystick::SdlGamepad;
use ai_pad_ctl::tray::TrayCommand;
use ai_pad_pet::PetWindow;

fn log(msg: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let ms = now.subsec_millis();
    println!("[{h:02}:{m:02}:{s:02}.{ms:03}] {msg}");
}

const VK_ALT: u16 = 0x12;
const VK_CTRL: u16 = 0x11;
const VK_TAB: u16 = 0x09;

struct HeldModifier {
    vk: u16,
    held: bool,
}

impl HeldModifier {
    fn new(vk: u16) -> Self {
        Self { vk, held: false }
    }
    fn press(&mut self) {
        if !self.held {
            let _ = ai_pad_core::hotkey::key_down(self.vk);
            self.held = true;
        }
        let _ = ai_pad_core::hotkey::key_down(VK_TAB);
        let _ = ai_pad_core::hotkey::key_up(VK_TAB);
    }
    fn release(&mut self) {
        if self.held {
            let _ = ai_pad_core::hotkey::key_up(self.vk);
            self.held = false;
        }
    }
}

fn exe_dir() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_else(|| ".".to_string())
}

fn resolve_config(filename: &str) -> String {
    let base = exe_dir();
    let exe_path = format!("{}\\{}", base, filename);
    if std::path::Path::new(&exe_path).exists() {
        exe_path
    } else {
        filename.to_string()
    }
}

fn check_single_instance() -> bool {
    use std::ptr::null_mut;
    use windows_sys::w;
    use windows_sys::core::PCWSTR;

    const MUTEX_NAME: PCWSTR = w!("ai-pad-ctl-single-instance-mutex");

    unsafe {
        let handle = CreateMutexW(null_mut(), 0, MUTEX_NAME);
        if handle.is_null() {
            return true;
        }
        let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if err == 183 {
            return false;
        }
        true
    }
}

fn main() {
    if !check_single_instance() {
        ai_pad_ctl::tray::show_error("ai-pad", "ai-pad 已在后台运行，请勿重复启动。");
        return;
    }

    let debug = std::env::args().any(|a| a == "--debug");
    if debug {
        unsafe { AllocConsole(); }
    }

    // pet 命令通道：gamepad → pet window
    let (pet_tx, pet_rx) = std::sync::mpsc::channel::<PetCommand>();
    // 托盘命令通道：tray → gamepad thread
    let (tray_tx, tray_rx) = std::sync::mpsc::channel::<TrayCommand>();

    // 后台线程：系统托盘消息循环
    let tray_handle = std::thread::spawn(move || {
        if let Err(e) = ai_pad_ctl::tray::run(tray_tx) {
            ai_pad_ctl::tray::show_error("ai-pad", &format!("托盘初始化失败: {e}"));
        }
    });

    // 后台线程：手柄轮询
    let gamepad_handle = std::thread::spawn(move || {
        gamepad_loop(&pet_tx, &tray_rx);
    });

    // 主线程：ggez 桌宠窗口（winit 要求主线程运行事件循环）
    if let Err(e) = PetWindow::run(pet_rx) {
        ai_pad_ctl::tray::show_error("ai-pad", &format!("桌宠窗口失败: {e}"));
    }

    let _ = tray_handle.join();
    let _ = gamepad_handle.join();
}

fn gamepad_loop(
    pet_tx: &std::sync::mpsc::Sender<PetCommand>,
    tray_rx: &std::sync::mpsc::Receiver<TrayCommand>,
) {
    let btn_config = match ButtonConfig::load(&resolve_config("buttons.yml")) {
        Ok(c) => c,
        Err(e) => {
            ai_pad_ctl::tray::show_error("ai-pad", &format!("无法加载 buttons.yml: {e}"));
            return;
        }
    };

    let action_config = match ActionConfig::load(&resolve_config("actions.yml")) {
        Ok(c) => c,
        Err(e) => {
            ai_pad_ctl::tray::show_error("ai-pad", &format!("无法加载 actions.yml: {e}"));
            return;
        }
    };

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
    } else {
        log("ai-pad 启动");
        for p in &pads {
            log(&format!("设备 [{}] {} ({} 按钮)", p.index, p.name, p.num_buttons));
        }
    }
    log(&format!("已加载 {} 个动作绑定", action_config.actions.len()));

    // AI Agent
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

    let mut gamepad = match SdlGamepad::open(&sdl, pads.first().map(|p| p.index).unwrap_or(0)) {
        Ok(g) => g,
        Err(e) => {
            log(&format!("打开手柄失败: {e}"));
            return;
        }
    };

    let mut prev_buttons: u32 = 0;
    let mut prev_hat: Option<(i32, i32)> = None;
    let mut alt_tab = HeldModifier::new(VK_ALT);
    let mut ctrl_tab = HeldModifier::new(VK_CTRL);

    loop {
        // 检查托盘命令
        if let Ok(cmd) = tray_rx.try_recv() {
            match cmd {
                TrayCommand::Exit => {
                    let _ = pet_tx.send(PetCommand::Exit);
                    break;
                }
                TrayCommand::Reload => {
                    log("重载配置...");
                    if let Ok(_c) = ButtonConfig::load(&resolve_config("buttons.yml")) {
                        log("重载配置完成");
                    }
                }
                TrayCommand::TogglePet => {
                    // 切换宠物状态：在 Happy 和 Idle 之间
                    let _ = pet_tx.send(PetCommand::SetState { state: ai_pad_core::bridge::PetStateName::Happy });
                }
            }
        }

        let buttons = gamepad.read_buttons();
        let new_presses = (buttons ^ prev_buttons) & buttons;
        if new_presses != 0 {
            for bit in 0..32 {
                if new_presses & (1 << bit) != 0 {
                    let idx = bit as usize;
                    let name = button_name(idx).unwrap_or("?");
                    let info = btn_config.get(idx as u32);
                    let display = info.map(|i| i.name.as_str()).unwrap_or(name);

                    if (alt_tab.held && display != "L1") || (ctrl_tab.held && display != "L2") {
                        alt_tab.release();
                        ctrl_tab.release();
                    }

                    log(&format!("按下 #{idx} {display}"));

                    // Bridge: 特殊按键 → AI / 宠物状态
                    let (agent_msg, pet_cmd) = handle_button_press(idx as u32, "");
                    if let Some(cmd) = pet_cmd {
                        log(&format!("  → Pet: {:?}", cmd));
                        if let Err(e) = pet_tx.send(cmd) {
                            log(&format!("发送宠物命令失败: {e}"));
                        }
                    }
                    if let (Some(msg), Some(ag)) = (agent_msg, &agent) {
                        log(&format!("  → AI 请求: {msg}"));
                        match rt.block_on(ag.chat(&msg)) {
                            Ok(reply) => {
                                log(&format!("  ← AI 回复: {}", &reply[..reply.len().min(60)]));
                                for cmd in resolve_agent_response(&reply) {
                                    if let Err(e) = pet_tx.send(cmd) {
                                        log(&format!("  发送失败: {e}"));
                                    }
                                }
                            }
                            Err(e) => {
                                log(&format!("  AI 错误: {e}"));
                            }
                        }
                    }

                    if let Some(action_def) = action_config.actions.get(display) {
                        log(&format!("  → {} ({})", display, action_def.action_type));
                        execute_action(action_def, &action_config.defaults, &mut alt_tab, &mut ctrl_tab);
                    }
                }
            }
        }
        prev_buttons = buttons;

        let hat = gamepad.read_hat(0);
        if let Some((dx, dy)) = hat {
            alt_tab.release();
            ctrl_tab.release();
            let scroll_speed = 3;
            if dy > 0 {
                let _ = ai_pad_core::hotkey::send_scroll(120 * scroll_speed);
            } else if dy < 0 {
                let _ = ai_pad_core::hotkey::send_scroll(-120 * scroll_speed);
            }
            if dx > 0 {
                let _ = ai_pad_core::hotkey::send_scroll_h(120 * scroll_speed);
            } else if dx < 0 {
                let _ = ai_pad_core::hotkey::send_scroll_h(-120 * scroll_speed);
            }
        }
        if hat != prev_hat {
            if let Some((dx, dy)) = hat {
                if let Some(h) = btn_config.hat.get(&(dx, dy)) {
                    log(&format!("滚动 {} {}", h.arrow, h.name));
                }
            }
            prev_hat = hat;
        }

        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    log("ai-pad 退出");
}

fn execute_action(
    action: &ai_pad_core::action::ActionDef,
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
            let workdir = if action.workdir.is_empty() {
                ".".to_string()
            } else {
                action.workdir.clone()
            };

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
                if let Err(e) = ai_pad_core::hotkey::trigger_hotkey(&key_refs, voice.delay) {
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
                    if let Err(e) = ai_pad_core::hotkey::trigger_hotkey(&key_refs, 0.02) {
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
