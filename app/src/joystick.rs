//! SDL2 游戏手柄读取模块（从 ctl/joystick.rs 搬移）

use sdl2::joystick::Joystick;
use sdl2::Sdl;

#[derive(Debug, Clone)]
pub struct GamepadInfo {
    pub index: u32,
    pub name: String,
    pub num_buttons: u32,
    pub num_axes: u32,
    pub num_hats: u32,
}

pub struct SdlGamepad {
    _sdl: Sdl,
    event_pump: sdl2::EventPump,
    joystick: Joystick,
    info: GamepadInfo,
}

impl SdlGamepad {
    pub fn init() -> Result<Sdl, String> {
        sdl2::init().map_err(|e| format!("SDL2 初始化失败: {e}"))
    }

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

    pub fn info(&self) -> &GamepadInfo {
        &self.info
    }

    pub fn read_buttons(&mut self) -> u32 {
        self.event_pump.pump_events();
        let mut buttons = 0u32;
        for i in 0..self.info.num_buttons.min(32) {
            if self.joystick.button(i).unwrap_or(false) {
                buttons |= 1 << i;
            }
        }
        buttons
    }

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
    #[ignore] // SDL2 在 cargo test 多线程环境下会 access violation，需要 --test-threads=1 或单独运行
    fn test_sdl_init_and_list() {
        let sdl = SdlGamepad::init().expect("SDL2 初始化");
        let pads = SdlGamepad::list_gamepads(&sdl).expect("列出设备");
        for p in &pads {
            println!("  [{}] {} ({} btn, {} hat)", p.index, p.name, p.num_buttons, p.num_hats);
        }
        assert!(SdlGamepad::open(&sdl, 99).is_err());
    }
}
