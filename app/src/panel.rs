//! 面板窗口模块：480×420 玻璃面板，3×3 网格 action 映射与方向键独占导航。
//!
//! 面板是按需创建的 WebView2 窗口，通过手柄方向键和 A/B 按钮进行独占导航
//! （导航期间手柄事件不传递给宠物状态机）。面板布局与动作从
//! `config/panel_action.yml` 加载，避免与手柄按键绑定共用 `config/actions.yml`。
//! 支持内置动作、启动程序和 PowerShell 脚本。
//!
//! 面板定位在宠物窗口附近（优先右下），超出屏幕时自动翻边。
//! 通过全局热键或 Home 键触发 `toggle_panel` 切换显示/隐藏。

use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

const PANEL_CONFIG_PATH: &str = "config/panel_action.yml";
const FALLBACK_PANEL_W: f64 = 480.0;
const FALLBACK_PANEL_H: f64 = 420.0;
const GAP: f64 = 10.0;

/// 获取面板渲染所需的布局和按钮列表。
#[tauri::command]
pub async fn cmd_get_panel_actions() -> Result<ai_pad_core::panel_action::PanelViewModel, String> {
    let config = load_panel_config()?;
    Ok(config.to_view_model())
}

/// 从 config/panel_action.yml 加载配置并执行面板动作（复用 core::action::launch_program）
#[tauri::command]
pub async fn cmd_execute_panel_action(id: String, app: AppHandle) -> Result<(), String> {
    let config = load_panel_config()?;
    let action_def = config
        .actions
        .get(&id)
        .ok_or_else(|| format!("未知动作: {id}"))?;

    if !action_def.enabled {
        return Err(format!("动作已禁用: {id}"));
    }

    match action_def.action_type.as_str() {
        "builtin" => match action_def.command.as_deref().unwrap_or(id.as_str()) {
            "dance" => {
                crate::action_bus::ActionBus::dispatch(
                    &app,
                    crate::action_bus::Action::PlayDance("happy_twist".into()),
                    crate::action_bus::ActionSource::Frontend {
                        cmd: "cmd_execute_panel_action:dance".into(),
                    },
                );
                hide_panel(&app)?;
                Ok(())
            }
            "game" => {
                hide_panel(&app)?;
                crate::action_bus::ActionBus::dispatch(
                    &app,
                    crate::action_bus::Action::PlayGameDefault,
                    crate::action_bus::ActionSource::Frontend {
                        cmd: "cmd_execute_panel_action:game".into(),
                    },
                );
                Ok(())
            }
            "settings" => {
                crate::settings::toggle_settings(&app);
                hide_panel(&app)?;
                Ok(())
            }
            "chat" => {
                crate::action_bus::ActionBus::dispatch(
                    &app,
                    crate::action_bus::Action::OpenChat,
                    crate::action_bus::ActionSource::Frontend {
                        cmd: "cmd_execute_panel_action:chat".into(),
                    },
                );
                hide_panel(&app)?;
                Ok(())
            }
            other => Err(format!("未知内置面板命令: {other}")),
        },
        "launch" => {
            let program = action_def.program.as_deref().ok_or("缺少 program")?;
            let args = action_def.args.as_deref().unwrap_or("");
            let result = ai_pad_core::action::launch_program(
                program,
                args,
                &action_def.workdir,
                action_def.terminal,
                &config.defaults.terminal,
            );
            if result.is_ok() {
                hide_panel(&app)?;
            }
            result
        }
        "script" => {
            if let Some(cmd) = &action_def.command {
                let result = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| format!("脚本执行失败: {e}"));
                if result.is_ok() {
                    hide_panel(&app)?;
                }
                result
            } else {
                Err("缺少 command".into())
            }
        }
        other => Err(format!("不支持的动作类型: {other}")),
    }
}

/// 调试用：前端通过此命令把日志转发到后端 stderr
#[tauri::command]
pub async fn cmd_panel_log(msg: String) -> Result<(), String> {
    let preview = ai_pad_core::logging::log_preview(&msg, 80);
    info!(
        msg_chars = msg.chars().count(),
        msg_preview = %preview,
        "[panel-js]"
    );
    Ok(())
}

/// 显示面板窗口并定位到宠物附近，由前端导航按键触发。
#[tauri::command]
pub async fn cmd_show_panel(app: AppHandle) -> Result<(), String> {
    show_panel(&app)
}

/// 显示面板窗口并定位到宠物附近，供 IPC、菜单和动作总线复用。
pub fn show_panel(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("panel") {
        apply_panel_size(&w);
        position_near_pet(app, &w);
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    } else {
        let w = create_panel_window(app).map_err(|e| e.to_string())?;
        position_near_pet(app, &w);
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 隐藏面板窗口，由前端导航取消或 B 按钮触发。
#[tauri::command]
pub async fn cmd_hide_panel(app: AppHandle) -> Result<(), String> {
    hide_panel(&app)
}

fn hide_panel(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("panel") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn load_panel_config() -> Result<ai_pad_core::panel_action::PanelActionConfig, String> {
    ai_pad_core::panel_action::PanelActionConfig::load(PANEL_CONFIG_PATH)
        .map_err(|e| format!("加载 {PANEL_CONFIG_PATH} 失败: {e}"))
}

fn panel_size_from_config() -> (f64, f64) {
    load_panel_config()
        .map(|config| {
            let vm = config.to_view_model();
            (vm.width as f64, vm.height as f64)
        })
        .unwrap_or((FALLBACK_PANEL_W, FALLBACK_PANEL_H))
}

/// 把 panel 窗口定位到宠物附近（优先右下，超出屏幕则自动换边）
fn position_near_pet(app: &AppHandle, panel: &tauri::WebviewWindow) {
    let Some(pet) = app.get_webview_window("pet") else {
        return;
    };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else {
        return;
    };
    let Some(monitor) = pet.current_monitor().ok().flatten() else {
        return;
    };
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    let scale = panel.scale_factor().unwrap_or(1.0);
    let (panel_w, panel_h) = panel_size_from_config();
    let pw = panel_w * scale;
    let ph = panel_h * scale;

    let screen_right = monitor_pos.x + monitor_size.width as i32 - pw as i32 - 8;
    let screen_bottom = monitor_pos.y + monitor_size.height as i32 - ph as i32 - 8;

    // 优先放右下
    let (mut px, mut py) = if pet_pos.x + pet_size.width as i32 + GAP as i32 + pw as i32
        <= monitor_pos.x + monitor_size.width as i32
    {
        (
            (pet_pos.x + pet_size.width as i32 + GAP as i32),
            (pet_pos.y + pet_size.height as i32 + GAP as i32),
        )
    } else {
        // 右边放不下 → 放左边
        (
            (pet_pos.x - GAP as i32 - pw as i32),
            (pet_pos.y + pet_size.height as i32 + GAP as i32),
        )
    };

    // 垂直方向：如果下方超出屏幕就放到上方
    if py > screen_bottom {
        py = (pet_pos.y as f64 - GAP - ph) as i32;
    }
    // 水平方向：确保不出界
    if px < monitor_pos.x {
        px = monitor_pos.x;
    }
    if px > screen_right {
        px = screen_right;
    }

    let _ = panel.set_position(PhysicalPosition::new(px, py));
}

/// 切换显示状态（全局热键 / Home 键调用）
pub fn toggle_panel(app: &AppHandle) {
    match app.get_webview_window("panel") {
        Some(w) => match w.is_visible() {
            Ok(true) => {
                info!("[panel] 隐藏");
                let _ = w.hide();
            }
            Ok(false) => {
                info!("[panel] 显示");
                apply_panel_size(&w);
                position_near_pet(app, &w);
                let _ = w.show();
                let _ = w.set_focus();
            }
            Err(e) => warn!(error = %e, "[panel] is_visible 错误"),
        },
        None => match create_panel_window(app) {
            Ok(w) => {
                position_near_pet(app, &w);
                info!("[panel] 已创建并显示");
                let _ = w.set_focus();
            }
            Err(e) => warn!(error = %e, "[panel] 创建失败"),
        },
    }
}

/// 按需创建 panel 窗口（默认可见、置中、聚焦）
fn create_panel_window(app: &AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    let (panel_w, panel_h) = panel_size_from_config();
    WebviewWindowBuilder::new(app, "panel", WebviewUrl::App("panel.html".into()))
        .title("8Bit Panel")
        .inner_size(panel_w, panel_h)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .center()
        .focused(true)
        .build()
}

fn apply_panel_size(panel: &tauri::WebviewWindow) {
    let (panel_w, panel_h) = panel_size_from_config();
    let _ = panel.set_size(LogicalSize::new(panel_w, panel_h));
}

#[cfg(test)]
mod tests {
    use super::PANEL_CONFIG_PATH;

    #[test]
    fn test_load_panel_actions_from_yml() {
        let config = ai_pad_core::panel_action::PanelActionConfig::load(PANEL_CONFIG_PATH).unwrap();
        assert!(config.actions.contains_key("vscode"));
        assert!(config.actions.contains_key("browser"));
        assert!(config.actions.contains_key("explorer"));
        assert!(config.actions.contains_key("powershell"));
        assert!(config.actions.contains_key("notepad"));
    }

    #[test]
    fn test_panel_actions_are_launch_type() {
        let config = ai_pad_core::panel_action::PanelActionConfig::load(PANEL_CONFIG_PATH).unwrap();
        let action = config.actions.get("vscode").unwrap();
        assert_eq!(action.action_type, "launch", "vscode 应为 launch 类型");
    }

    #[test]
    fn test_unknown_action_errors() {
        let config = ai_pad_core::panel_action::PanelActionConfig::load(PANEL_CONFIG_PATH).unwrap();
        assert!(config.actions.get("nonexistent").is_none());
    }
}
