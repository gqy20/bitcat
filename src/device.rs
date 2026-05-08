use gilrs::{Gilrs, Button};

/// 查找已连接的手柄，返回名称和 ID
pub fn find_gamepad(gilrs: &Gilrs) -> Option<(gilrs::GamepadId, String)> {
    for (id, gamepad) in gilrs.gamepads() {
        let name = gamepad.name().to_lowercase();
        // 8BitDo 设备通常包含 "8bitdo" 或 "bluetooth" 关键词
        if name.contains("8bitdo")
            || name.contains("bluetooth wireless controller")
            || name.contains("gamepad")
            || name.contains("xinput")
        {
            return Some((id, gamepad.name().to_string()));
        }
    }
    None
}

/// 列出所有已连接的游戏手柄
pub fn list_gamepads(gilrs: &Gilrs) -> Vec<(gilrs::GamepadId, String)> {
    gilrs.gamepads()
        .map(|(id, gp)| (id, gp.name().to_string()))
        .collect()
}

/// 按钮事件
#[derive(Debug, Clone, PartialEq)]
pub enum PadEvent {
    ButtonPressed { name: String, id: usize },
    ButtonReleased { name: String, id: usize },
    HatChanged { arrow: String, dx: i32, dy: i32 },
    Connected { name: String },
    Disconnected,
}

/// 按钮索引 → 名称（与 buttons.yml 对应）
pub fn button_name(index: usize) -> Option<&'static str> {
    match index {
        0 => Some("A"),
        1 => Some("B"),
        3 => Some("X"),
        4 => Some("Y"),
        6 => Some("L1"),
        7 => Some("R1"),
        8 => Some("L2"),
        9 => Some("R2"),
        10 => Some("Select"),
        11 => Some("Start"),
        12 => Some("Home"),
        _ => None,
    }
}

/// gilrs Button → 按钮索引
pub fn gilrs_button_index(btn: Button) -> Option<usize> {
    match btn {
        Button::South => Some(0),     // A
        Button::East => Some(1),      // B
        Button::West => Some(3),      // X
        Button::North => Some(4),     // Y
        Button::LeftTrigger => Some(6),  // L1
        Button::RightTrigger => Some(7), // R1
        Button::LeftTrigger2 => Some(8), // L2
        Button::RightTrigger2 => Some(9),// R2
        Button::Select => Some(10),
        Button::Start => Some(11),
        Button::Mode => Some(12),     // Home
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_name_known() {
        assert_eq!(button_name(0), Some("A"));
        assert_eq!(button_name(1), Some("B"));
        assert_eq!(button_name(4), Some("Y"));
        assert_eq!(button_name(11), Some("Start"));
        assert_eq!(button_name(12), Some("Home"));
    }

    #[test]
    fn test_button_name_unknown() {
        assert_eq!(button_name(2), None);
        assert_eq!(button_name(99), None);
    }

    #[test]
    fn test_gilrs_button_mapping() {
        assert_eq!(gilrs_button_index(Button::South), Some(0));
        assert_eq!(gilrs_button_index(Button::East), Some(1));
        assert_eq!(gilrs_button_index(Button::West), Some(3));
        assert_eq!(gilrs_button_index(Button::North), Some(4));
        assert_eq!(gilrs_button_index(Button::Select), Some(10));
        assert_eq!(gilrs_button_index(Button::Start), Some(11));
        assert_eq!(gilrs_button_index(Button::Mode), Some(12));
    }

    #[test]
    fn test_gilrs_unmapped_button() {
        assert_eq!(gilrs_button_index(Button::C), None);
        assert_eq!(gilrs_button_index(Button::Unknown), None);
    }

    #[test]
    fn test_list_gamepads() {
        // 在 CI / 无手柄环境下，列表应该为空或不 panic
        if let Ok(gilrs) = Gilrs::new() {
            let _pads = list_gamepads(&gilrs);
            // 不 assert 数量，因为取决于运行环境
        }
    }

    #[test]
    fn test_find_gamepad_no_panic() {
        if let Ok(gilrs) = Gilrs::new() {
            let _result = find_gamepad(&gilrs);
        }
    }

    #[test]
    fn test_pad_event_debug() {
        let evt = PadEvent::ButtonPressed {
            name: "A".into(),
            id: 0,
        };
        assert!(format!("{:?}", evt).contains("ButtonPressed"));
    }
}
