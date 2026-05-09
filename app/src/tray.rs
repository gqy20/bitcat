use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter,
};
use crate::gamepad::PetEvent;

/// 托盘菜单项 ID
const MENU_RELOAD: &str = "reload";
const MENU_TOGGLE: &str = "toggle";
const MENU_EXIT: &str = "exit";

pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let reload_i = MenuItem::with_id(app, MENU_RELOAD, "Reload Config", true, None::<&str>)?;
    let toggle_i = MenuItem::with_id(app, MENU_TOGGLE, "Toggle Pet", true, None::<&str>)?;
    let exit_i = MenuItem::with_id(app, MENU_EXIT, "Exit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&reload_i, &toggle_i, &exit_i])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            MENU_RELOAD => {
                // 重载配置（Phase 7 实现）
            }
            MENU_TOGGLE => {
                let event = PetEvent::set_state("happy");
                let _ = app.emit("pet-event", &event);
            }
            MENU_EXIT => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_ids_are_unique() {
        // 确保菜单 ID 不重复
        let ids = [MENU_RELOAD, MENU_TOGGLE, MENU_EXIT];
        let mut sorted = ids;
        sorted.sort();
        for i in 0..sorted.len() - 1 {
            assert_ne!(sorted[i], sorted[i + 1], "duplicate menu id: {}", sorted[i]);
        }
    }

    #[test]
    fn test_menu_ids_are_non_empty() {
        assert!(!MENU_RELOAD.is_empty());
        assert!(!MENU_TOGGLE.is_empty());
        assert!(!MENU_EXIT.is_empty());
    }

    #[test]
    fn test_toggle_event_is_happy() {
        let event = PetEvent::set_state("happy");
        assert_eq!(event.state, Some("happy".to_string()));
    }

    #[test]
    fn test_pet_event_for_exit() {
        let event = PetEvent::set_state("exit");
        assert_eq!(event.state, Some("exit".to_string()));
    }
}
