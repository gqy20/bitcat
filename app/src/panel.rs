use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

/// 从 actions.yml 加载配置并执行面板动作（复用 core::action::launch_program）
#[tauri::command]
pub async fn cmd_execute_panel_action(id: String) -> Result<(), String> {
    let config = ai_pad_core::action::ActionConfig::load("actions.yml")
        .map_err(|e| format!("加载 actions.yml 失败: {e}"))?;
    let action_def = config.actions.get(&id)
        .ok_or_else(|| format!("未知动作: {id}"))?;

    match action_def.action_type.as_str() {
        "launch" => {
            let program = action_def.program.as_deref().ok_or("缺少 program")?;
            let args = action_def.args.as_deref().unwrap_or("");
            ai_pad_core::action::launch_program(
                program, args,
                &action_def.workdir,
                action_def.terminal,
                &config.defaults.terminal,
            )
        }
        "script" => {
            if let Some(cmd) = &action_def.command {
                std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| format!("脚本执行失败: {e}"))
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
    info!(msg = %msg, "[panel-js]");
    Ok(())
}

#[tauri::command]
pub async fn cmd_show_panel(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("panel") {
        position_near_pet(&app, &w);
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_hide_panel(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("panel") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

const PANEL_W: f64 = 480.0;
const PANEL_H: f64 = 320.0;
const GAP: f64 = 10.0;

/// 把 panel 窗口定位到宠物附近（优先右下，超出屏幕则自动换边）
fn position_near_pet(app: &AppHandle, panel: &tauri::WebviewWindow) {
    let Some(pet) = app.get_webview_window("pet") else { return };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else { return };
    let Some(monitor) = pet.current_monitor().ok().flatten() else { return };
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    let scale = panel.scale_factor().unwrap_or(1.0);
    let pw = PANEL_W * scale;
    let ph = PANEL_H * scale;

    let screen_right = monitor_pos.x + monitor_size.width as i32 - pw as i32 - 8;
    let screen_bottom = monitor_pos.y + monitor_size.height as i32 - ph as i32 - 8;

    // 优先放右下
    let (mut px, mut py) = if pet_pos.x + pet_size.width as i32 + GAP as i32 + pw as i32 <= monitor_pos.x + monitor_size.width as i32 {
        ((pet_pos.x + pet_size.width as i32 + GAP as i32), (pet_pos.y + pet_size.height as i32 + GAP as i32))
    } else {
        // 右边放不下 → 放左边
        ((pet_pos.x - GAP as i32 - pw as i32), (pet_pos.y + pet_size.height as i32 + GAP as i32))
    };

    // 垂直方向：如果下方超出屏幕就放到上方
    if py > screen_bottom {
        py = (pet_pos.y as f64 - GAP as f64 - ph) as i32;
    }
    // 水平方向：确保不出界
    if px < monitor_pos.x { px = monitor_pos.x; }
    if px > screen_right { px = screen_right; }

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
    WebviewWindowBuilder::new(app, "panel", WebviewUrl::App("panel.html".into()))
        .title("8Bit Panel")
        .inner_size(480.0, 320.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_panel_actions_from_yml() {
        let config = ai_pad_core::action::ActionConfig::load("actions.yml").unwrap();
        // 面板动作都在 actions.yml 中了
        assert!(config.actions.contains_key("vscode"));
        assert!(config.actions.contains_key("browser"));
        assert!(config.actions.contains_key("explorer"));
        assert!(config.actions.contains_key("powershell"));
        assert!(config.actions.contains_key("notepad"));
    }

    #[test]
    fn test_panel_actions_are_launch_type() {
        let config = ai_pad_core::action::ActionConfig::load("actions.yml").unwrap();
        for key in ["vscode", "browser", "notepad"] {
            let action = config.actions.get(*key).unwrap();
            assert_eq!(action.action_type, "launch", "{key} 应为 launch 类型");
        }
    }

    #[test]
    fn test_unknown_action_errors() {
        let config = ai_pad_core::action::ActionConfig::load("actions.yml").unwrap();
        assert!(config.actions.get("nonexistent").is_none());
    }
}
