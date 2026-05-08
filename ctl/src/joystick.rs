//! SDL2 游戏手柄读取模块
//! 和 Python pygame 使用完全相同的后端（SDL2 DirectInput）

use sdl2::joystick::Joystick;
use sdl2::Sdl;

/// 已连接的游戏手柄信息
#[derive(Debug, Clone)]
pub struct GamepadInfo {
    pub index: u32,
    pub name: String,
    pub num_buttons: u32,
    pub num_axes: u32,
    pub num_hats: u32,
}

/// SDL2 手柄读取器
pub struct SdlGamepad {
    _sdl: Sdl,
    event_pump: sdl2::EventPump,
    joystick: Joystick,
    info: GamepadInfo,
}

impl SdlGamepad {
    /// 初始化 SDL2
    pub fn init() -> Result<Sdl, String> {
        sdl2::init().map_err(|e| format!("SDL2 初始化失败: {e}"))
    }

    /// 列出所有游戏手柄
    pub fn list_gamepads(sdl: &Sdl) -> Result<Vec<GamepadInfo>, String> {
        let subsystem = sdl.joystick().map_err(|e| format!("Joystick 子系统失败: {e}"))?;
        let mut result = Vec::new();

        let count = subsystem.num_joysticks().map_err(|e| format!("获取数量失败: {e}"))?;
        for i in 0..count {
            if let Ok(joy) = subsystem.open(i as u32) {
                result.push(GamepadInfo {
                    index: i as u32,
                    name: joy.name(),
                    num_buttons: joy.num_buttons(),
                    num_axes: joy.num_axes(),
                    num_hats: joy.num_hats(),
                });
            }
        }
        Ok(result)
    }

    /// 打开指定手柄
    pub fn open(sdl: &Sdl, index: u32) -> Result<Self, String> {
        let subsystem = sdl.joystick().map_err(|e| format!("Joystick 子系统失败: {e}"))?;
        let joystick = subsystem.open(index)
            .map_err(|e| format!("打开手柄 {index} 失败: {e}"))?;
        let event_pump = sdl.event_pump()
            .map_err(|e| format!("EventPump 初始化失败: {e}"))?;

        let info = GamepadInfo {
            index,
            name: joystick.name(),
            num_buttons: joystick.num_buttons(),
            num_axes: joystick.num_axes(),
            num_hats: joystick.num_hats(),
        };

        Ok(Self { _sdl: sdl.clone(), event_pump, joystick, info })
    }

    /// 获取设备信息
    pub fn info(&self) -> &GamepadInfo {
        &self.info
    }

    /// 读取所有按钮状态，返回位掩码
    pub fn read_buttons(&mut self) -> u32 {
        // 必须先 pump events，SDL2 才会更新 joystick 状态
        self.event_pump.pump_events();
        let mut buttons = 0u32;
        for i in 0..self.info.num_buttons.min(32) {
            if self.joystick.button(i).unwrap_or(false) {
                buttons |= 1 << i;
            }
        }
        buttons
    }

    /// 读取方向键状态
    pub fn read_hat(&mut self, hat_index: u32) -> Option<(i32, i32)> {
        let state = self.joystick.hat(hat_index).ok()?;
        use sdl2::joystick::HatState;
        match state {
            HatState::Up => Some((0, 1)),
            HatState::RightUp => Some((1, 1)),
            HatState::Right => Some((1, 0)),
            HatState::RightDown => Some((1, -1)),
            HatState::Down => Some((0, -1)),
            HatState::LeftDown => Some((-1, -1)),
            HatState::Left => Some((-1, 0)),
            HatState::LeftUp => Some((-1, 1)),
            HatState::Centered => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sdl_init_and_list() {
        // SDL2 不支持多线程并发初始化，合并为一个测试
        let sdl = SdlGamepad::init().expect("SDL2 初始化");
        let pads = SdlGamepad::list_gamepads(&sdl).expect("列出设备");
        for p in &pads {
            println!("  [{}] {} ({} btn, {} hat)", p.index, p.name, p.num_buttons, p.num_hats);
        }
        assert!(SdlGamepad::open(&sdl, 99).is_err());
    }
}
