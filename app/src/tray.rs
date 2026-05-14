use crate::commands::SharedWindowState;
use std::sync::atomic::Ordering;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tracing::info;

const MENU_SCREENSHOT: &str = "screenshot";
const MENU_COLLAPSE: &str = "collapse";
const MENU_TOP: &str = "top";
const MENU_RELOAD: &str = "reload";
const MENU_SETTINGS: &str = "settings";
const MENU_EXIT: &str = "exit";

pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let settings_item = MenuItem::with_id(app, MENU_SETTINGS, "设置…", true, None::<&str>)?;
    let screenshot_item =
        MenuItem::with_id(app, MENU_SCREENSHOT, "分析当前屏幕", true, None::<&str>)?;
    let separator_primary = PredefinedMenuItem::separator(app)?;
    let collapse_item = MenuItem::with_id(app, MENU_COLLAPSE, "折叠", true, None::<&str>)?;
    let top_item = MenuItem::with_id(app, MENU_TOP, "取消置顶", true, None::<&str>)?;
    let separator_state = PredefinedMenuItem::separator(app)?;
    let reload_item = MenuItem::with_id(app, MENU_RELOAD, "重新载入配置", true, None::<&str>)?;
    let exit_item = MenuItem::with_id(app, MENU_EXIT, "退出 8Bit Cat", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &settings_item,
            &screenshot_item,
            &separator_primary,
            &collapse_item,
            &top_item,
            &separator_state,
            &reload_item,
            &exit_item,
        ],
    )?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            MENU_SCREENSHOT => {
                let app_clone = app.clone();
                std::thread::spawn(move || {
                    if let Err(e) = crate::screenshot::do_screenshot_now(&app_clone) {
                        tracing::warn!(error = %e, "托盘截图失败");
                    }
                });
            }
            MENU_COLLAPSE => {
                let ws: tauri::State<'_, SharedWindowState> = app.state();
                let now_collapsed = !ws.collapsed.load(Ordering::SeqCst);
                ws.collapsed.store(now_collapsed, Ordering::SeqCst);

                // 折叠前保存当前窗口位置，展开时用此坐标重建
                if now_collapsed {
                    if let Some(win) = app.get_webview_window("pet") {
                        if let Ok(pos) = win.outer_position() {
                            crate::snap::remember_pet_position(&ws, pos.x, pos.y);
                            info!(x = pos.x, y = pos.y, "保存折叠前位置");
                        }
                    }
                }

                let label = if now_collapsed { "展开" } else { "折叠" };
                info!(collapsed = now_collapsed, "托盘: {}", label);
                let _ = collapse_item.set_text(label);
                let _ = app.emit("pet-toggle-collapse", now_collapsed);
            }
            MENU_TOP => {
                let ws: tauri::State<'_, SharedWindowState> = app.state();
                let now_top = !ws.always_on_top.load(Ordering::SeqCst);
                ws.always_on_top.store(now_top, Ordering::SeqCst);

                let label = if now_top { "取消置顶" } else { "置顶" };
                info!(always_on_top = now_top, "托盘: {}", label);
                let _ = top_item.set_text(label);

                if let Some(win) = app.get_webview_window("pet") {
                    let _ = win.set_always_on_top(now_top);
                }
            }
            MENU_RELOAD => {
                ws_reload_config(app);
            }
            MENU_SETTINGS => {
                crate::settings::toggle_settings(app);
            }
            MENU_EXIT => {
                crate::shutdown::request_exit(app, "tray-exit");
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// 重载 config/actions.yml + config/prompts.yml，通知 gamepad_loop 刷新
fn ws_reload_config(app: &AppHandle) {
    match ai_pad_core::action::ActionConfig::load("config/actions.yml") {
        Ok(cfg) => tracing::info!(actions = cfg.actions.len(), "已重载 config/actions.yml"),
        Err(e) => tracing::warn!(error = %e, "重载 config/actions.yml 失败"),
    }
    let _ = ai_pad_core::prompts::PromptsConfig::load();
    tracing::info!("已重载 config/prompts.yml");

    let ws: tauri::State<'_, SharedWindowState> = app.state();
    ws.config_reload.store(true, Ordering::SeqCst);
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_ids_are_unique() {
        let ids = [
            MENU_SCREENSHOT,
            MENU_COLLAPSE,
            MENU_TOP,
            MENU_RELOAD,
            MENU_SETTINGS,
            MENU_EXIT,
        ];
        let mut sorted = ids;
        sorted.sort();
        for i in 0..sorted.len() - 1 {
            assert_ne!(sorted[i], sorted[i + 1], "duplicate menu id: {}", sorted[i]);
        }
    }

    #[test]
    fn test_menu_ids_are_non_empty() {
        assert!(!MENU_SCREENSHOT.is_empty());
        assert!(!MENU_COLLAPSE.is_empty());
        assert!(!MENU_TOP.is_empty());
        assert!(!MENU_RELOAD.is_empty());
        assert!(!MENU_SETTINGS.is_empty());
        assert!(!MENU_EXIT.is_empty());
    }

    #[test]
    fn test_window_state_defaults() {
        let ws = SharedWindowState::default();
        assert!(!ws.collapsed.load(Ordering::SeqCst));
        assert!(ws.always_on_top.load(Ordering::SeqCst));
        assert!(!ws.config_reload.load(Ordering::SeqCst));
    }
}
