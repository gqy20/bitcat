use ai_pad::config::ButtonConfig;
use ai_pad::action::ActionConfig;
use ai_pad::device::button_name;
use ai_pad::joystick::SdlGamepad;

fn main() {
    let btn_config = match ButtonConfig::load("buttons.yml") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("无法加载 buttons.yml: {e}");
            std::process::exit(1);
        }
    };

    let action_config = match ActionConfig::load("actions.yml") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("无法加载 actions.yml: {e}");
            std::process::exit(1);
        }
    };

    let sdl = match SdlGamepad::init() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let pads = match SdlGamepad::list_gamepads(&sdl) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if pads.is_empty() {
        eprintln!("未检测到游戏手柄");
        std::process::exit(1);
    }

    println!("ai-pad-ctl: AI 驱动的手柄控制器");
    println!("已连接: {} 个设备", pads.len());
    for p in &pads {
        println!("  [{}] {} ({} 按钮)", p.index, p.name, p.num_buttons);
    }
    println!("已加载 {} 个动作绑定", action_config.actions.len());
    println!("按 Ctrl+C 退出");
    println!("---");

    let mut gamepad = match SdlGamepad::open(&sdl, pads[0].index) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut prev_buttons: u32 = 0;
    let mut prev_hat: Option<(i32, i32)> = None;

    loop {
        let buttons = gamepad.read_buttons();
        let new_presses = (buttons ^ prev_buttons) & buttons;
        if new_presses != 0 {
            for bit in 0..32 {
                if new_presses & (1 << bit) != 0 {
                    let idx = bit as usize;
                    let name = button_name(idx).unwrap_or("?");
                    let info = btn_config.get(idx as u32);
                    let display = info.map(|i| i.name.as_str()).unwrap_or(name);
                    println!("[按下] #{idx} {display}");

                    if let Some(action_def) = action_config.actions.get(display) {
                        println!("  → 触发动作: {} ({})", display, action_def.action_type);
                        execute_action(action_def, &action_config.defaults);
                    }
                }
            }
        }
        prev_buttons = buttons;

        // 方向键 → 持续滚动
        let hat = gamepad.read_hat(0);
        if let Some((dx, dy)) = hat {
            let scroll_speed = 3;
            if dy > 0 {
                let _ = ai_pad::hotkey::send_scroll(120 * scroll_speed);
            } else if dy < 0 {
                let _ = ai_pad::hotkey::send_scroll(-120 * scroll_speed);
            }
            if dx > 0 {
                let _ = ai_pad::hotkey::send_scroll_h(120 * scroll_speed);
            } else if dx < 0 {
                let _ = ai_pad::hotkey::send_scroll_h(-120 * scroll_speed);
            }
        }
        if hat != prev_hat {
            if let Some((dx, dy)) = hat {
                if let Some(h) = btn_config.hat.get(&(dx, dy)) {
                    println!("[滚动] {} {}", h.arrow, h.name);
                }
            }
            prev_hat = hat;
        }

        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

fn execute_action(action: &ai_pad::action::ActionDef, defaults: &ai_pad::action::Defaults) {
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
                println!("  执行: {full_args}");
                let _ = std::process::Command::new("powershell")
                    .args(["-Command", &full_args])
                    .spawn();
            } else {
                println!("  执行: {program} {args}");
                let _ = std::process::Command::new(&program)
                    .args(args.split_whitespace())
                    .current_dir(&workdir)
                    .spawn();
            }
        }
        "voice" => {
            if let Some(voice) = &action.voice {
                let key_refs: Vec<&str> = voice.trigger.iter().map(|s| s.as_str()).collect();
                println!("  语音触发: 按键 {} 延迟 {}s", key_refs.join("+"), voice.delay);
                if let Err(e) = ai_pad::hotkey::trigger_hotkey(&key_refs, voice.delay) {
                    eprintln!("  热键触发失败: {e}");
                }
            }
        }
        "script" => {
            if let Some(cmd) = &action.command {
                println!("  执行脚本: {cmd}");
                let _ = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn();
            }
        }
        other => {
            println!("  未知动作类型: {other}");
        }
    }
}
