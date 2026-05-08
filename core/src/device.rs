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
}
