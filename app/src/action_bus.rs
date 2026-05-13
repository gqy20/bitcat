//! ActionBus：三路输入归一层
//!
//! 手柄 / 键盘全局热键 / 前端命令三路只负责"触发"，业务语义统一在 Bus 里落地。
//!
//! 当前 B1 阶段仅提供类型骨架 + `dispatch` 日志，**不接入**任何调用方。
//! B2-B4 会把各输入源接到 [`ActionBus::dispatch`]。

use ai_pad_core::action::ActionDef;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use tauri::AppHandle;
use tracing::{debug, info, warn};

// ---- Action 类型 ----

/// 调节键 + Tab 的按住态类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierKind {
    Alt,
    Ctrl,
}

/// 业务语义动作。
///
/// 每种变体代表一种"用户意图"，与具体触发源无关。
/// 详见 [plan B1](file:///C:/Users/gqy17/AppData/Roaming/Qoder/SharedClientCache/cache/plans/ActionBus_三路输入归一_5d6a0942.md)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// 切换面板显隐
    TogglePanel,
    /// 打开对话输入（创建 bubble 窗口 + showInput）
    OpenChat,
    /// 退出对话（截图恢复写 bubble）
    ExitChat,
    /// 提交一条对话消息（写入 SharedPendingChat，由 chat_loop 消费）
    SubmitChat(String),
    /// 触发语音录音条（预留给后续输入路径）
    StartVoice,
    /// 播放舞蹈
    PlayDance(String),
    /// 立即截图 + Vision 分析
    ScreenshotNow,
    /// 启动程序（launch 动作）
    Launch {
        program: String,
        args: String,
        workdir: String,
        terminal: bool,
    },
    /// 纯键盘组合热键（非 Tab 按住态）
    Hotkey(Vec<String>),
    /// Alt+Tab / Ctrl+Tab 的按住态切换
    ModifierTab(ModifierKind),
    /// PowerShell 脚本
    Script(String),
    /// 兜底扩展
    Custom(String),
}

/// 动作触发源。用于日志 / 审计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionSource {
    Gamepad {
        button: String,
    },
    Keyboard {
        shortcut: String,
    },
    /// 前端 Tauri cmd 名；允许 `&'static str` 字面量或反序列化时的 owned `String`。
    Frontend {
        cmd: Cow<'static, str>,
    },
    Internal,
}

// ---- Bus 本体 ----

/// 动作总线（当前无状态，留作后续审计队列的扩展点）。
pub struct ActionBus;

impl ActionBus {
    /// 把 `ActionDef`（yml 解析结果）翻译为强类型 [`Action`]。
    ///
    /// 只处理**无状态**动作；`voice` 返回 `None`（由 gamepad 物理层管理）；
    /// `hotkey` 若是 Alt+Tab / Ctrl+Tab 组合，返回 `ModifierTab`；其他返回 `Hotkey`。
    pub fn from_def(def: &ActionDef) -> Option<Action> {
        match def.action_type.as_str() {
            "launch" => {
                let program = def.program.clone()?;
                Some(Action::Launch {
                    program,
                    args: def.args.clone().unwrap_or_default(),
                    workdir: def.workdir.clone(),
                    terminal: def.terminal,
                })
            }
            "script" => def.command.clone().map(Action::Script),
            "hotkey" => {
                let trigger = def.trigger.as_ref()?;
                let lc: Vec<String> = trigger.iter().map(|s| s.to_lowercase()).collect();
                let has_alt = lc.iter().any(|k| k == "alt");
                let has_ctrl = lc.iter().any(|k| k == "ctrl");
                let has_tab = lc.iter().any(|k| k == "tab");
                if has_tab && has_alt {
                    Some(Action::ModifierTab(ModifierKind::Alt))
                } else if has_tab && has_ctrl {
                    Some(Action::ModifierTab(ModifierKind::Ctrl))
                } else {
                    Some(Action::Hotkey(trigger.clone()))
                }
            }
            // voice 是物理按住态，需要 gamepad 层直接管理，不进 Bus
            "voice" => None,
            _ => None,
        }
    }

    /// 分发一个动作。
    ///
    /// B1 阶段仅打日志；后续 Task（B2-B5）逐步接入真实路由。
    #[allow(unused_variables)]
    pub fn dispatch(app: &AppHandle, action: Action, source: ActionSource) {
        debug!(?source, ?action, "[action-bus] dispatch");
        match &action {
            Action::TogglePanel => {
                info!(?source, "[action-bus] → TogglePanel");
                crate::panel::toggle_panel(app);
            }
            Action::ExitChat => {
                info!(?source, "[action-bus] → ExitChat");
                let state: tauri::State<'_, crate::bubble::SharedBubble> =
                    tauri::Manager::state(app);
                state.set_chat_active(false);
            }
            Action::OpenChat => {
                info!(?source, "[action-bus] → OpenChat");
                open_chat_impl(app);
            }
            Action::SubmitChat(text) => {
                info!(?source, len = text.len(), "[action-bus] → SubmitChat");
                submit_chat_impl(app, text.clone());
            }
            Action::PlayDance(name) => {
                info!(?source, dance = %name, "[action-bus] → PlayDance");
                play_dance_impl(name.clone());
            }
            Action::ScreenshotNow => {
                info!(?source, "[action-bus] → ScreenshotNow");
                if let Err(e) = crate::screenshot::do_screenshot_now(app) {
                    warn!(error = %e, "[action-bus] 截图失败");
                }
            }
            Action::Launch {
                program,
                args,
                workdir,
                terminal,
            } => {
                info!(?source, program = %program, "[action-bus] → Launch");
                if let Err(e) = ai_pad_core::action::launch_program(
                    program,
                    args,
                    workdir,
                    *terminal,
                    "powershell",
                ) {
                    warn!(error = %e, "[action-bus] 启动程序失败");
                }
            }
            Action::Script(cmd) => {
                info!(?source, "[action-bus] → Script");
                if let Err(e) = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn()
                {
                    warn!(error = %e, "[action-bus] 启动脚本失败");
                }
            }
            Action::Hotkey(keys) => {
                info!(?source, keys = ?keys, "[action-bus] → Hotkey");
                let refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
                if let Err(e) = ai_pad_core::hotkey::trigger_hotkey(&refs, 0.02) {
                    warn!(error = %e, "[action-bus] 热键触发失败");
                }
            }
            Action::ModifierTab(kind) => {
                // 按住态由 gamepad 物理层管理，Bus 只留日志（不做实际按键）。
                info!(
                    ?source,
                    ?kind,
                    "[action-bus] → ModifierTab（由调用方接管按住态）"
                );
            }
            Action::StartVoice => {
                // 语音录音条开启由 gamepad 物理层管理（HeldCombo 按住态）；
                // Bus 仅作为预留扩展点，后续如果加"前端/键盘触发一次按住态"再实现。
                info!(?source, "[action-bus] → StartVoice（预留）");
            }
            Action::Custom(tag) => {
                info!(?source, tag = %tag, "[action-bus] → Custom");
            }
        }
    }
}

// ---- 私有实现（把既有 cmd 的业务逻辑抽到 Bus） ----

fn submit_chat_impl(app: &AppHandle, text: String) {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        warn!("[action-bus] SubmitChat 文本为空，跳过");
        return;
    }
    let state: tauri::State<'_, crate::gamepad::SharedPendingChat> = tauri::Manager::state(app);
    if let Err(e) = state.set(trimmed) {
        warn!(error = %e, "[action-bus] SharedPendingChat 写入失败");
    }
}

fn open_chat_impl(app: &AppHandle) {
    use tauri::Manager;

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => match crate::bubble::create_bubble_window(app) {
            Ok(w) => w,
            Err(e) => {
                warn!(error = %e, "[action-bus] 创建 bubble 窗口失败");
                return;
            }
        },
    };

    let state: tauri::State<'_, crate::bubble::SharedBubble> = app.state();
    if let Ok(mut g) = state.pending_text.lock() {
        *g = Some(String::new());
    }
    state.set_chat_active(true);

    crate::bubble::position_above_pet(app, &window);
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();

    for attempt in 0..10u8 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        if window
            .eval(
                "if(typeof __bubble_showInput==='function'){__bubble_showInput();'ok'}else{'no-fn'}",
            )
            .is_ok()
        {
            info!(attempt, "[action-bus] ✓ eval showInput 成功");
            return;
        }
    }
    warn!("[action-bus] eval showInput 失败（10 次重试均未成功）");
}

fn play_dance_impl(dance_name: String) {
    if ai_pad_core::dance::load_dance(&dance_name).is_err() {
        warn!(dance = %dance_name, "[action-bus] 舞蹈不存在，无法播放");
        return;
    }

    let req = ai_pad_core::dance::PlayDanceRequest {
        name: dance_name,
        loops: Some(1),
        duration_ms: None,
    };
    if let Err(e) = ai_pad_core::dance::request_play_dance(req) {
        warn!(error = %e, "[action-bus] request_play_dance 失败");
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use ai_pad_core::action::ActionDef;

    fn def(ty: &str) -> ActionDef {
        ActionDef {
            action_type: ty.into(),
            program: None,
            args: None,
            workdir: String::new(),
            terminal: false,
            command: None,
            voice: None,
            trigger: None,
            keyboard_shortcut: None,
        }
    }

    #[test]
    fn action_debug_and_serde_roundtrip() {
        let actions = [
            Action::TogglePanel,
            Action::OpenChat,
            Action::ExitChat,
            Action::SubmitChat("你好".into()),
            Action::PlayDance("happy".into()),
            Action::ScreenshotNow,
            Action::Launch {
                program: "code".into(),
                args: String::new(),
                workdir: String::new(),
                terminal: false,
            },
            Action::Hotkey(vec!["ctrl".into(), "c".into()]),
            Action::ModifierTab(ModifierKind::Alt),
            Action::Script("Get-Process".into()),
            Action::Custom("foo".into()),
        ];
        for a in &actions {
            let s = serde_json::to_string(a).expect("serialize");
            let back: Action = serde_json::from_str(&s).expect("deserialize");
            assert_eq!(&back, a, "roundtrip fail: {s}");
            // Debug 不 panic
            let _ = format!("{a:?}");
        }
    }

    #[test]
    fn source_serde_roundtrip() {
        let sources = [
            ActionSource::Gamepad { button: "A".into() },
            ActionSource::Keyboard {
                shortcut: "Ctrl+Alt+D".into(),
            },
            ActionSource::Frontend {
                cmd: "cmd_play_dance".into(),
            },
            ActionSource::Internal,
        ];
        for s in &sources {
            let json = serde_json::to_string(s).expect("ser");
            let back: ActionSource = serde_json::from_str(&json).expect("de");
            assert_eq!(&back, s);
        }
    }

    #[test]
    fn from_def_launch() {
        let mut d = def("launch");
        d.program = Some("claude".into());
        d.args = Some("--foo".into());
        d.workdir = "C:\\".into();
        d.terminal = true;
        let a = ActionBus::from_def(&d).expect("launch");
        assert_eq!(
            a,
            Action::Launch {
                program: "claude".into(),
                args: "--foo".into(),
                workdir: "C:\\".into(),
                terminal: true,
            }
        );
    }

    #[test]
    fn from_def_launch_missing_program() {
        let d = def("launch");
        assert!(ActionBus::from_def(&d).is_none());
    }

    #[test]
    fn from_def_script() {
        let mut d = def("script");
        d.command = Some("Write-Host hi".into());
        assert_eq!(
            ActionBus::from_def(&d),
            Some(Action::Script("Write-Host hi".into()))
        );
    }

    #[test]
    fn from_def_hotkey_plain() {
        let mut d = def("hotkey");
        d.trigger = Some(vec!["ctrl".into(), "c".into()]);
        assert_eq!(
            ActionBus::from_def(&d),
            Some(Action::Hotkey(vec!["ctrl".into(), "c".into()]))
        );
    }

    #[test]
    fn from_def_hotkey_alt_tab() {
        let mut d = def("hotkey");
        d.trigger = Some(vec!["Alt".into(), "Tab".into()]);
        assert_eq!(
            ActionBus::from_def(&d),
            Some(Action::ModifierTab(ModifierKind::Alt))
        );
    }

    #[test]
    fn from_def_hotkey_ctrl_tab() {
        let mut d = def("hotkey");
        d.trigger = Some(vec!["CTRL".into(), "TAB".into()]);
        assert_eq!(
            ActionBus::from_def(&d),
            Some(Action::ModifierTab(ModifierKind::Ctrl))
        );
    }

    #[test]
    fn from_def_voice_returns_none() {
        let d = def("voice");
        assert!(ActionBus::from_def(&d).is_none());
    }

    #[test]
    fn from_def_unknown_returns_none() {
        let d = def("mystery");
        assert!(ActionBus::from_def(&d).is_none());
    }
}
