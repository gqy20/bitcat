//! 快捷键模拟模块测试
//!
//! TDD: 先定义期望的按键映射行为

use std::collections::HashMap;

/// 按键名 → Windows Virtual Key Code
pub fn parse_keys(keys: &[&str]) -> Vec<u16> {
    static MAP: &[(&str, u16)] = &[
        ("ctrl", 0x11), ("control", 0x11),
        ("win", 0x5B), ("windows", 0x5B),
        ("alt", 0x12),
        ("shift", 0x10),
        ("enter", 0x0D), ("return", 0x0D),
        ("tab", 0x09), ("esc", 0x1B), ("space", 0x20),
        ("a", 0x41), ("b", 0x42), ("c", 0x43), ("d", 0x44),
    ];
    let map: HashMap<&str, u16> = MAP.iter().cloned().collect();

    keys.iter().map(|k| {
        *map.get(k.to_lowercase().as_str()).unwrap_or(&0)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ctrl_win() {
        let codes = parse_keys(&["ctrl", "win"]);
        assert_eq!(codes, vec![0x11, 0x5B]);
    }

    #[test]
    fn test_parse_single_key() {
        assert_eq!(parse_keys(&["enter"]), vec![0x0D]);
        assert_eq!(parse_keys(&["space"]), vec![0x20]);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(parse_keys(&["Ctrl"]), vec![0x11]);
        assert_eq!(parse_keys(&["WIN"]), vec![0x5B]);
    }

    #[test]
    fn test_parse_unknown_key() {
        assert_eq!(parse_keys(&["unknown"]), vec![0]);
    }

    #[test]
    fn test_parse_empty() {
        let codes = parse_keys(&[]);
        assert!(codes.is_empty());
    }
}
