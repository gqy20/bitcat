use crate::commands::SharedWindowState;
use std::sync::atomic::Ordering;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, LogicalPosition, Manager, WebviewWindow,
};
use tracing::info;

const MENU_SCREENSHOT: &str = "screenshot";
const MENU_COLLAPSE: &str = "collapse";
const MENU_TOP: &str = "top";
const MENU_RELOAD: &str = "reload";
const MENU_SETTINGS: &str = "settings";
const MENU_EXIT: &str = "exit";

const PET_MENU_CHAT: &str = "pet-context-chat";
const PET_MENU_SCREENSHOT: &str = "pet-context-screenshot";
const PET_MENU_PANEL: &str = "pet-context-panel";
const PET_MENU_COLLAPSE: &str = "pet-context-collapse";
const PET_MENU_TOP: &str = "pet-context-top";
const PET_MENU_MORE: &str = "pet-context-more";
const PET_MENU_RELOAD: &str = "pet-context-reload";
const PET_MENU_SETTINGS: &str = "pet-context-settings";
const PET_MENU_EXIT: &str = "pet-context-exit";

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
                analyze_current_screen(app);
            }
            MENU_COLLAPSE => {
                let now_collapsed = toggle_pet_collapse(app);
                let label = if now_collapsed { "展开" } else { "折叠" };
                let _ = collapse_item.set_text(label);
            }
            MENU_TOP => {
                let now_top = toggle_pet_always_on_top(app);
                let label = if now_top { "取消置顶" } else { "置顶" };
                let _ = top_item.set_text(label);
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

pub fn handle_pet_context_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        PET_MENU_CHAT => open_chat(app),
        PET_MENU_SCREENSHOT => analyze_current_screen(app),
        PET_MENU_PANEL => {
            if let Err(e) = crate::panel::show_panel(app) {
                tracing::warn!(error = %e, "打开快捷面板失败");
            }
        }
        PET_MENU_COLLAPSE => {
            let _ = toggle_pet_collapse(app);
        }
        PET_MENU_TOP => {
            let _ = toggle_pet_always_on_top(app);
        }
        PET_MENU_RELOAD => ws_reload_config(app),
        PET_MENU_SETTINGS => crate::settings::toggle_settings(app),
        PET_MENU_EXIT => crate::shutdown::request_exit(app, "pet-context-exit"),
        _ => {}
    }
}

#[tauri::command]
pub async fn cmd_show_pet_context_menu(
    window: WebviewWindow,
    app: AppHandle,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let collapsed = ws.collapsed.load(Ordering::SeqCst);
    let always_on_top = ws.always_on_top.load(Ordering::SeqCst);

    let chat_item = MenuItem::with_id(&app, PET_MENU_CHAT, "和我说话", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let screenshot_item = MenuItem::with_id(
        &app,
        PET_MENU_SCREENSHOT,
        "观察当前屏幕",
        true,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    let panel_item = MenuItem::with_id(&app, PET_MENU_PANEL, "打开快捷面板", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let separator_primary = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let collapse_item = CheckMenuItem::with_id(
        &app,
        PET_MENU_COLLAPSE,
        "折叠显示",
        true,
        collapsed,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    let top_item = CheckMenuItem::with_id(
        &app,
        PET_MENU_TOP,
        "保持置顶",
        true,
        always_on_top,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    let separator_state = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
    let settings_item = MenuItem::with_id(&app, PET_MENU_SETTINGS, "设置…", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let reload_item = MenuItem::with_id(&app, PET_MENU_RELOAD, "重新载入配置", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let more_menu = Submenu::with_id_and_items(&app, PET_MENU_MORE, "更多", true, &[&reload_item])
        .map_err(|e| e.to_string())?;
    let exit_item = MenuItem::with_id(&app, PET_MENU_EXIT, "退出 8Bit Cat", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(
        &app,
        &[
            &chat_item,
            &screenshot_item,
            &panel_item,
            &separator_primary,
            &collapse_item,
            &top_item,
            &separator_state,
            &settings_item,
            &more_menu,
            &exit_item,
        ],
    )
    .map_err(|e| e.to_string())?;

    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn open_chat(app: &AppHandle) {
    crate::action_bus::ActionBus::dispatch(
        app,
        crate::action_bus::Action::OpenChat,
        crate::action_bus::ActionSource::Frontend {
            cmd: "pet-context-chat".into(),
        },
    );
}

fn analyze_current_screen(app: &AppHandle) {
    let app_clone = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = crate::screenshot::do_screenshot_now(&app_clone) {
            tracing::warn!(error = %e, "截图失败");
        }
    });
}

fn toggle_pet_collapse(app: &AppHandle) -> bool {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let now_collapsed = !ws.collapsed.load(Ordering::SeqCst);
    ws.collapsed.store(now_collapsed, Ordering::SeqCst);

    // 折叠前保存当前窗口位置，展开时用此坐标重建。
    if now_collapsed {
        if let Some(win) = visible_pet_window(app) {
            if let Ok(pos) = win.outer_position() {
                crate::snap::remember_pet_position(&ws, pos.x, pos.y);
                info!(x = pos.x, y = pos.y, "保存折叠前位置");
            }
        }
    }

    let label = if now_collapsed { "展开" } else { "折叠" };
    info!(collapsed = now_collapsed, "宠物菜单: {}", label);
    let _ = app.emit("pet-toggle-collapse", now_collapsed);
    now_collapsed
}

fn toggle_pet_always_on_top(app: &AppHandle) -> bool {
    let ws: tauri::State<'_, SharedWindowState> = app.state();
    let now_top = !ws.always_on_top.load(Ordering::SeqCst);
    ws.always_on_top.store(now_top, Ordering::SeqCst);

    let label = if now_top { "取消置顶" } else { "置顶" };
    info!(always_on_top = now_top, "宠物菜单: {}", label);
    for label in ["pet", "pet-mini", "pet-snap"] {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_always_on_top(now_top);
        }
    }
    let _ = app.emit("pet-toggle-top", now_top);
    now_top
}

fn visible_pet_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| {
            app.get_webview_window("pet-mini")
                .filter(|w| w.is_visible().unwrap_or(false))
        })
        .or_else(|| {
            app.get_webview_window("pet-snap")
                .filter(|w| w.is_visible().unwrap_or(false))
        })
}

/// 重载 config/actions.yml / config/panel_action.yml / config/prompts.yml，通知 gamepad_loop 刷新。
fn ws_reload_config(app: &AppHandle) {
    match ai_pad_core::action::ActionConfig::load("config/actions.yml") {
        Ok(cfg) => tracing::info!(actions = cfg.actions.len(), "已重载 config/actions.yml"),
        Err(e) => tracing::warn!(error = %e, "重载 config/actions.yml 失败"),
    }
    match ai_pad_core::panel_action::PanelActionConfig::load("config/panel_action.yml") {
        Ok(cfg) => {
            tracing::info!(
                actions = cfg.actions.len(),
                "已重载 config/panel_action.yml"
            )
        }
        Err(e) => tracing::warn!(error = %e, "重载 config/panel_action.yml 失败"),
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
            PET_MENU_CHAT,
            PET_MENU_SCREENSHOT,
            PET_MENU_PANEL,
            PET_MENU_COLLAPSE,
            PET_MENU_TOP,
            PET_MENU_MORE,
            PET_MENU_RELOAD,
            PET_MENU_SETTINGS,
            PET_MENU_EXIT,
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
