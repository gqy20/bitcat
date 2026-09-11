//! opencode 插件安装器。
//!
//! opencode 通过 `~/.config/opencode/plugins/*.js` 自动发现全局插件（Linux /
//! macOS / WSL 用 `~/.config`，Windows 原生可用 `%APPDATA%`，均支持
//! `XDG_CONFIG_HOME`），不需要改 opencode.json。本模块只负责原子写入
//! BitCat 的只读转发插件；协议细节见
//! docs/research/pi-opencode-agent-watch-protocol.md。

use crate::agent_monitor::DEFAULT_AGENT_MONITOR_PORT;
use crate::hook_install_common::{atomic_write, file_content_matches};
use std::path::PathBuf;
use tauri_plugin_opener::OpenerExt;

const BITCAT_PLUGIN_MARKER: &str = "bitcat-opencode-watch";

/// opencode 全局配置目录。已存在的目录优先（opencode 初始化时已创建），
/// 否则按平台约定选第一个可用候选。
pub fn opencode_config_dir() -> Result<PathBuf, String> {
    let candidates = [
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(|v| PathBuf::from(v).join("opencode")),
        std::env::var_os("APPDATA")
            .filter(|v| !v.is_empty())
            .map(|v| PathBuf::from(v).join("opencode")),
        home_dir().map(|dir| dir.join(".config").join("opencode")),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.is_dir() {
            return Ok(candidate.clone());
        }
    }
    candidates
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "无法解析 opencode 配置目录".to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

pub fn plugin_path() -> Result<PathBuf, String> {
    Ok(opencode_config_dir()?
        .join("plugins")
        .join("bitcat-watch.js"))
}

pub fn install_opencode_plugin() -> Result<String, String> {
    let path = plugin_path()?;
    let script = plugin_script(DEFAULT_AGENT_MONITOR_PORT);
    let updated = !file_content_matches(&path, &script);
    atomic_write(&path, &script)?;
    Ok(format!(
        "opencode 插件就绪：{}（{}）",
        path.display(),
        if updated { "已更新" } else { "无变更" }
    ))
}

#[tauri::command]
pub async fn cmd_install_opencode_plugin() -> Result<String, String> {
    install_opencode_plugin()
}

#[tauri::command]
pub async fn cmd_open_opencode_plugins_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = plugin_path()?;
    let dir = dir
        .parent()
        .ok_or_else(|| "无法解析插件目录".to_string())?
        .to_path_buf();
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| e.to_string())
}

/// opencode 插件脚本：
/// - `event` 钩子转发关心的会话事件（流式增量在插件侧过滤）；
/// - `tool.execute.before/after` 是独立 hook 不进 event 流，合成为同构事件；
/// - 维护 sessionID → directory 缓存，因为 tool hook 里拿不到会话目录。
fn plugin_script(port: u16) -> String {
    r#"// __MARKER__ — Installed by BitCat. Read-only session watcher.
// 协议: docs/research/pi-opencode-agent-watch-protocol.md
import { createConnection } from "node:net";
import { hostname } from "node:os";

const PORT = __PORT__;
const WATCH_EVENTS = new Set([
  "session.created",
  "session.updated",
  "session.status",
  "session.idle",
  "session.error",
  "session.deleted",
  "session.compacted",
  "message.part.updated",
  "permission.asked",
  "permission.replied",
]);

const directories = new Map();

function send(event, directory) {
  try {
    const envelope = {
      schema: "bitcat.agent-hook.v1",
      source: "opencode",
      machine: hostname(),
      payload: {
        schema: "bitcat-opencode-watch/1",
        event,
        directory,
      },
    };
    const socket = createConnection(PORT, "127.0.0.1");
    socket.on("error", () => {});
    socket.on("connect", () => {
      socket.end(JSON.stringify(envelope), () => socket.destroy());
    });
  } catch {
    // 只读观察：转发失败不影响 opencode。
  }
}

export const BitcatWatch = async ({ directory }) => {
  return {
    event: async ({ event }) => {
      if (!event || !WATCH_EVENTS.has(event.type)) return;
      const props = event.properties ?? {};
      const sessionID = props.sessionID ?? props.sessionId;
      if (!sessionID) return;
      const infoDirectory = props.info?.directory;
      if (infoDirectory) directories.set(sessionID, infoDirectory);
      send(event, infoDirectory ?? directories.get(sessionID) ?? directory);
    },
    "tool.execute.before": async (input, output) => {
      const dir = directories.get(input.sessionID) ?? directory;
      send(
        {
          id: "bitcat-" + input.callID + "-before",
          type: "tool.execute.before",
          properties: {
            sessionID: input.sessionID,
            tool: input.tool,
            args: output?.args,
          },
        },
        dir,
      );
      return output;
    },
    "tool.execute.after": async (input) => {
      const dir = directories.get(input.sessionID) ?? directory;
      send(
        {
          id: "bitcat-" + input.callID + "-after",
          type: "tool.execute.after",
          properties: {
            sessionID: input.sessionID,
            tool: input.tool,
            args: input.args,
          },
        },
        dir,
      );
    },
  };
};
"#
    .replace("__PORT__", &port.to_string())
    .replace("__MARKER__", BITCAT_PLUGIN_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_script_targets_monitor_and_marks_source() {
        let script = plugin_script(5342);
        assert!(script.contains(BITCAT_PLUGIN_MARKER));
        assert!(script.contains("const PORT = 5342;"));
        assert!(script.contains("127.0.0.1"));
        assert!(script.contains("source: \"opencode\""));
        assert!(script.contains("session.status"));
        assert!(script.contains("tool.execute.before"));
        assert!(script.contains("tool.execute.after"));
        assert!(script.contains("permission.asked"));
    }

    #[test]
    fn config_dir_prefers_existing_xdg_directory_from_env() {
        let base = std::env::temp_dir().join(format!(
            "bitcat-oc-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let opencode_dir = base.join("opencode");
        std::fs::create_dir_all(&opencode_dir).unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        let old_appdata = std::env::var_os("APPDATA");
        std::env::set_var("XDG_CONFIG_HOME", &base);
        std::env::remove_var("APPDATA");
        let dir = opencode_config_dir().unwrap();
        // 恢复环境，避免污染其他测试。
        match old {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        if let Some(v) = old_appdata {
            std::env::set_var("APPDATA", v);
        }
        assert_eq!(dir, opencode_dir);
        std::fs::remove_dir_all(&base).ok();
    }

    /// 显式运行（`cargo test -- --ignored`）时写入真实 opencode 插件，供端到端联调。
    #[test]
    #[ignore = "写入真实用户目录，仅手动联调时运行"]
    fn e2e_install_real_plugin() {
        let report = install_opencode_plugin().unwrap();
        println!("{report}");
        assert!(plugin_path().unwrap().exists());
    }
}
