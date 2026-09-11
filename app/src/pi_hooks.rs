//! pi 扩展安装器。
//!
//! pi 通过 `~/.pi/agent/extensions/*.ts` 自动发现全局扩展（支持
//! `PI_CODING_AGENT_DIR` 覆盖），不需要改任何用户配置文件。本模块只负责
//! 原子写入 BitCat 的只读转发扩展；协议细节见
//! docs/research/pi-opencode-agent-watch-protocol.md。

use crate::agent_monitor::DEFAULT_AGENT_MONITOR_PORT;
use crate::hook_install_common::{atomic_write, file_content_matches};
use std::path::PathBuf;
use tauri_plugin_opener::OpenerExt;

const BITCAT_EXTENSION_MARKER: &str = "bitcat-pi-watch";

/// pi agent 配置目录：`PI_CODING_AGENT_DIR` 或 `~/.pi/agent`。
pub fn pi_agent_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("PI_CODING_AGENT_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    home_dir()
        .map(|dir| dir.join(".pi").join("agent"))
        .ok_or_else(|| "无法解析 home 目录".to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

pub fn extension_path() -> Result<PathBuf, String> {
    Ok(pi_agent_dir()?.join("extensions").join("bitcat-watch.ts"))
}

pub fn install_pi_extension() -> Result<String, String> {
    let path = extension_path()?;
    let script = extension_script(DEFAULT_AGENT_MONITOR_PORT);
    let updated = !file_content_matches(&path, &script);
    atomic_write(&path, &script)?;
    Ok(format!(
        "pi 扩展就绪：{}（{}）",
        path.display(),
        if updated { "已更新" } else { "无变更" }
    ))
}

#[tauri::command]
pub async fn cmd_install_pi_extension() -> Result<String, String> {
    install_pi_extension()
}

#[tauri::command]
pub async fn cmd_open_pi_extensions_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = extension_path()?;
    let dir = dir
        .parent()
        .ok_or_else(|| "无法解析扩展目录".to_string())?
        .to_path_buf();
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

/// pi 扩展脚本：订阅生命周期事件，一次性 TCP 连接转发到 Agent Watch monitor。
///
/// pi 约束 extension factory 不得启动后台资源，因此这里每个事件独立建连、
/// 发完即销毁，失败静默，绝不阻塞 pi 本身。
fn extension_script(port: u16) -> String {
    r#"// __MARKER__ — Installed by BitCat. Read-only session watcher.
// 协议: docs/research/pi-opencode-agent-watch-protocol.md
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { createConnection } from "node:net";
import { hostname } from "node:os";

const PORT = __PORT__;
const EVENTS = [
  "session_start",
  "before_agent_start",
  "agent_start",
  "agent_settled",
  "tool_execution_start",
  "tool_execution_end",
  "message_end",
  "session_compact",
  "session_shutdown",
] as const;

export default function (pi: ExtensionAPI) {
  const send = (name: string, data: unknown, ctx: any) => {
    try {
      const envelope = {
        schema: "bitcat.agent-hook.v1",
        source: "pi",
        machine: hostname(),
        payload: {
          schema: "bitcat-pi-watch/1",
          event: name,
          session_id: ctx?.sessionManager?.getSessionId?.(),
          cwd: ctx?.cwd,
          data,
        },
      };
      const body = JSON.stringify(envelope);
      const socket = createConnection(PORT, "127.0.0.1");
      socket.on("error", () => {});
      socket.on("connect", () => {
        socket.end(body, () => socket.destroy());
      });
    } catch {
      // 只读观察：转发失败不影响 pi。
    }
  };
  for (const name of EVENTS) {
    pi.on(name as any, ((event: any, ctx: any) => {
      send(name, event, ctx);
    }) as any);
  }
}
"#
    .replace("__PORT__", &port.to_string())
    .replace("__MARKER__", BITCAT_EXTENSION_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_script_targets_monitor_and_marks_source() {
        let script = extension_script(5342);
        assert!(script.contains(BITCAT_EXTENSION_MARKER));
        assert!(script.contains("const PORT = 5342;"));
        assert!(script.contains("127.0.0.1"));
        assert!(script.contains("source: \"pi\""));
        assert!(script.contains("session_start"));
        assert!(script.contains("agent_settled"));
        assert!(script.contains("tool_execution_start"));
        assert!(script.contains("session_shutdown"));
        // pi 约束：factory 不得启动后台资源，脚本内不能有模块级 socket。
        assert!(!script.contains("createServer"));
    }

    #[test]
    fn extension_path_respects_pi_agent_dir_override() {
        // 不依赖真实环境：只在变量已设置时验证行为，避免测试间污染。
        if std::env::var_os("PI_CODING_AGENT_DIR").is_some() {
            let path = extension_path().unwrap();
            assert!(path.starts_with(std::env::var_os("PI_CODING_AGENT_DIR").unwrap()));
        }
    }

    /// 显式运行（`cargo test -- --ignored`）时写入真实 ~/.pi 扩展，供端到端联调。
    #[test]
    #[ignore = "写入真实用户目录，仅手动联调时运行"]
    fn e2e_install_real_extension() {
        let report = install_pi_extension().unwrap();
        println!("{report}");
        assert!(extension_path().unwrap().exists());
    }
}
