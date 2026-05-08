fn main() {
    let sdl = sdl2::init().expect("SDL2 init failed");
    let gc_subsystem = sdl.game_controller().expect("GameController subsystem failed");
    let joy_subsystem = sdl.joystick().expect("Joystick subsystem failed");

    let count = joy_subsystem.num_joysticks().unwrap_or(0);
    println!("=== 震动支持测试 ===");
    println!("检测到 {count} 个 joystick 设备\n");

    for i in 0..count {
        // 先用 Joystick 获取设备名
        let name = match joy_subsystem.open(i) {
            Ok(joy) => {
                let n = joy.name();
                drop(joy); // 关闭，让后续可以重新打开
                n
            }
            Err(_) => format!("设备 #{i}"),
        };

        let is_gc = gc_subsystem.is_game_controller(i);
        println!("[{i}] {name}");
        println!("    GameController 映射: {}", if is_gc { "✅ 是" } else { "❌ 否" });

        if is_gc {
            let mut controller = match gc_subsystem.open(i) {
                Ok(c) => c,
                Err(e) => { println!("    打开失败: {e}"); continue; }
            };

            let has_rumble = controller.has_rumble();
            let has_rumble_triggers = controller.has_rumble_triggers();

            println!("    震动 (主马达): {}", if has_rumble { "✅ 支持" } else { "❌ 不支持" });
            println!("    扳机震动:      {}", if has_rumble_triggers { "✅ 支持" } else { "❌ 不支持" });

            if has_rumble {
                println!("\n    >>> 尝试触发震动测试 (1秒)... <<<");
                let _ = controller.set_rumble(0xFFFF, 0xFFFF, 500);
                std::thread::sleep(std::time::Duration::from_secs(1));
                println!("    >>> 测试完成 <<<");

                println!("\n    >>> 弱震动测试 (1秒)... <<<");
                let _ = controller.set_rumble(0x4000, 0x2000, 800);
                std::thread::sleep(std::time::Duration::from_secs(1));
                println!("    >>> 测试完成 <<<");
            } else {
                println!("\n    ⚠️  当前模式不支持震动");
                println!("    提示: XInput 模式或有线连接可能支持");
            }
        } else {
            println!("    ⚠️  未识别为标准 GameController，无法查询震动能力");
        }
        println!();
    }

    if count == 0 {
        println!("未检测到任何手柄设备！");
        println!("请确认: 手柄已开启、模式开关在 D 挡、蓝牙已配对");
    }
}
