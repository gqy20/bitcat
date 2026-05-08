use ai_pad_core::config::ButtonConfig;
use ai_pad_core::device::button_name;
use ai_pad_ctl::joystick::SdlGamepad;

fn main() {
    let btn_config = match ButtonConfig::load("buttons.yml") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("无法加载 buttons.yml: {e}");
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
        eprintln!("请确认: 1.手柄已开启 2.模式开关在 D 挡 3.蓝牙已配对");
        std::process::exit(1);
    }

    println!("=== ai-pad-read: 按键测试工具 ===");
    println!("检测到 {} 个手柄:", pads.len());
    for p in &pads {
        println!("  [{}] {} ({} 按钮, {} 方向键)", p.index, p.name, p.num_buttons, p.num_hats);
    }
    println!();

    // 按钮映射表
    for id in [0, 1, 3, 4, 6, 7, 8, 9, 10, 11, 12] {
        if let Some(info) = btn_config.get(id) {
            println!("  #{id:2} {name:<6} {pos}", name = info.name, pos = info.position);
        }
    }
    println!();
    println!("提示: {}", btn_config.dpad_hint);
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
        let changed = buttons ^ prev_buttons;
        if changed != 0 {
            for bit in 0..32 {
                if changed & (1 << bit) != 0 {
                    let pressed = buttons & (1 << bit) != 0;
                    let idx = bit as usize;
                    let name = button_name(idx).unwrap_or("?");
                    let info = btn_config.get(idx as u32);
                    let display = info.map(|i| i.name.as_str()).unwrap_or(name);

                    if pressed {
                        let aliases = info.map(|i| i.aliases.join(", ")).unwrap_or_default();
                        let position = info.map(|i| i.position.as_str()).unwrap_or("未知");
                        println!("[按下] #{idx} {display}  位置:{position}  别名:{aliases}");
                    } else {
                        println!("[释放] #{idx} {display}");
                    }
                }
            }
            prev_buttons = buttons;
        }

        // 方向键
        let hat = gamepad.read_hat(0);
        if hat != prev_hat {
            if let Some((dx, dy)) = hat {
                if let Some(h) = btn_config.hat.get(&(dx, dy)) {
                    println!("[方向] {} {}", h.arrow, h.name);
                }
            }
            prev_hat = hat;
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
