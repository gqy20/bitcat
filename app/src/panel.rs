use tauri::{AppHandle, Manager};

/// Phase A: 硬编码动作映射 (id -> (program, args, terminal))
///
/// Phase B 会从 panel.yml 加载，这里先写死。
pub fn lookup_action(id: &str) -> Option<(&'static str, &'static [&'static str], bool)> {
    match id {
        "vscode"     => Some(("code",         &[],                                    false)),
        "browser"    => Some(("explorer",     &["https://www.bing.com"],              false)),
        "explorer"   => Some(("explorer.exe", &["."],                                 false)),
        "powershell" => Some(("pwsh",         &[],                                    true)),
        "notepad"    => Some(("notepad.exe",  &[],                                    false)),
        _ => None,
    }
}

fn spawn(program: &str, args: &[&str], terminal: bool) -> Result<(), String> {
    if terminal {
        let cmd = if args.is_empty() {
            program.to_string()
        } else {
            format!("{} {}", program, args.join(" "))
        };
        let full = format!(
            "Start-Process pwsh -ArgumentList '-NoExit','-Command','{cmd}' -WindowStyle Maximized"
        );
        std::process::Command::new("powershell")
            .args(["-Command", &full])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("启动失败: {e}"))
    } else {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("启动失败: {e}"))
    }
}

#[tauri::command]
pub async fn cmd_execute_panel_action(id: String) -> Result<(), String> {
    let (program, args, terminal) = lookup_action(&id)
        .ok_or_else(|| format!("未知 action_id: {id}"))?;
    spawn(program, args, terminal)
}

#[tauri::command]
pub async fn cmd_show_panel(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("panel") {
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

/// 切换显示状态（全局热键调用）
pub fn toggle_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        match w.is_visible() {
            Ok(true) => {
                eprintln!("[panel] 隐藏");
                let _ = w.hide();
            }
            Ok(false) => {
                eprintln!("[panel] 显示");
                let _ = w.show();
                let _ = w.set_focus();
            }
            Err(e) => {
                eprintln!("[panel] is_visible 错误: {e}");
            }
        }
    } else {
        eprintln!("[panel] panel 窗口不存在");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_known_actions() {
        assert!(lookup_action("vscode").is_some());
        assert!(lookup_action("browser").is_some());
        assert!(lookup_action("explorer").is_some());
        assert!(lookup_action("powershell").is_some());
        assert!(lookup_action("notepad").is_some());
    }

    #[test]
    fn test_lookup_unknown_action() {
        assert!(lookup_action("not_a_real_action").is_none());
        assert!(lookup_action("").is_none());
    }

    #[test]
    fn test_powershell_is_terminal() {
        let (_, _, terminal) = lookup_action("powershell").unwrap();
        assert!(terminal, "PowerShell 必须在终端中运行");
    }

    #[test]
    fn test_vscode_not_terminal() {
        let (_, _, terminal) = lookup_action("vscode").unwrap();
        assert!(!terminal);
    }

    #[test]
    fn test_explorer_has_args() {
        let (program, args, _) = lookup_action("explorer").unwrap();
        assert_eq!(program, "explorer.exe");
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_browser_opens_url() {
        let (_, args, _) = lookup_action("browser").unwrap();
        assert!(args[0].starts_with("https://"));
    }
}
