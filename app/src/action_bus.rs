//! ActionBus：三路输入归一层。
//!
//! 手柄按钮、键盘全局热键、前端 Tauri 命令三路输入只负责"触发"，
//! 业务语义统一在 [`ActionBus::dispatch`] 中落地（开面板、发消息、截图、启动程序等）。
//!
//! 这样设计是为了让 gamepad_loop 不再直接耦合各业务的实现细节，新增动作类型时
//! 只需扩展 [`Action`] 枚举和 `dispatch` 的 match 分支。
//! 各输入源通过 [`ActionSource`] 标记来源，便于日志审计和问题排查。

use bitcat_core::action::ActionDef;
use bitcat_core::logging::log_preview;
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
    /// 启动默认迷你游戏
    PlayGameDefault,
    /// Start the built-in memory matching mode.
    PlayMemoryDefault,
    /// Start the built-in falling-catch mode.
    PlayCatchDefault,
    /// 启动默认守护召唤战
    PlayBattleDefault,
    /// Start the built-in AI Gomoku mode.
    PlayGomokuDefault,
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

/// 动作触发源。用于日志审计和问题排查。
///
/// 每个 `dispatch` 调用都会携带来源信息，便于追溯某个动作是手柄按钮触发、
/// 键盘热键触发、前端命令触发还是内部系统触发。
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

/// 动作总线：三路输入的统一调度入口。
///
/// 当前无内部状态，所有路由逻辑在 [`dispatch`](ActionBus::dispatch) 中完成。
/// 预留为后续审计队列或动作重放机制的扩展点。
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
            "screenshot" | "screenshot_now" => Some(Action::ScreenshotNow),
            _ => None,
        }
    }

    /// 分发一个动作：根据 [`Action`] 变体执行对应的业务逻辑。
    ///
    /// 每个 match 分支负责一种动作类型：面板切换、对话开启/退出、截图、
    /// 程序启动、脚本执行、热键触发等。所有分支都会记录带 `source` 的结构化日志。
    #[allow(unused_variables)]
    pub fn dispatch(app: &AppHandle, action: Action, source: ActionSource) {
        debug!(?source, ?action, "action dispatch");
        match &action {
            Action::TogglePanel => {
                info!(?source, action = "TogglePanel", "action dispatch");
                crate::panel::toggle_panel(app);
            }
            Action::ExitChat => {
                info!(?source, action = "ExitChat", "action dispatch");
                let state: tauri::State<'_, crate::bubble::SharedBubble> =
                    tauri::Manager::state(app);
                state.set_chat_active(false);
            }
            Action::OpenChat => {
                info!(?source, action = "OpenChat", "action dispatch");
                open_chat_impl(app);
            }
            Action::SubmitChat(text) => {
                let text_preview = log_preview(text, 80);
                info!(
                    ?source,
                    action = "SubmitChat",
                    text_chars = text.chars().count(),
                    text_preview = %text_preview,
                    "action dispatch"
                );
                submit_chat_impl(app, text.clone());
            }
            Action::PlayDance(name) => {
                info!(?source, action = "PlayDance", dance = %name, "action dispatch");
                play_dance_impl(name.clone());
            }
            Action::PlayGameDefault => {
                info!(?source, action = "PlayGameDefault", "action dispatch");
                if let Err(e) = crate::game::start_default_game(app) {
                    warn!(error = %e, "play game action failed");
                }
            }
            Action::PlayMemoryDefault => {
                info!(?source, action = "PlayMemoryDefault", "action dispatch");
                if let Err(e) = crate::game::start_default_memory(app) {
                    warn!(error = %e, "play memory action failed");
                }
            }
            Action::PlayCatchDefault => {
                info!(?source, action = "PlayCatchDefault", "action dispatch");
                if let Err(e) = crate::game::start_default_catch(app) {
                    warn!(error = %e, "play catch action failed");
                }
            }
            Action::PlayBattleDefault => {
                info!(?source, action = "PlayBattleDefault", "action dispatch");
                if let Err(e) = crate::game::start_default_battle(app) {
                    warn!(error = %e, "play battle action failed");
                }
            }
            Action::PlayGomokuDefault => {
                info!(?source, action = "PlayGomokuDefault", "action dispatch");
                if let Err(e) = crate::game::start_default_gomoku(app) {
                    warn!(error = %e, "play gomoku action failed");
                }
            }
            Action::ScreenshotNow => {
                info!(?source, action = "ScreenshotNow", "action dispatch");
                let app = app.clone();
                std::thread::spawn(move || {
                    if let Err(e) = crate::screenshot::do_screenshot_now(&app) {
                        warn!(error = %e, "screenshot action failed");
                    }
                });
            }
            Action::Launch {
                program,
                args,
                workdir,
                terminal,
            } => {
                info!(?source, action = "Launch", program = %program, "action dispatch");
                if let Err(e) = bitcat_core::action::launch_program(
                    program,
                    args,
                    workdir,
                    *terminal,
                    "powershell",
                ) {
                    warn!(error = %e, "launch action failed");
                }
            }
            Action::Script(cmd) => {
                let command_preview = log_preview(cmd, 80);
                info!(
                    ?source,
                    action = "Script",
                    command_chars = cmd.chars().count(),
                    command_preview = %command_preview,
                    "action dispatch"
                );
                if let Err(e) = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn()
                {
                    warn!(error = %e, "script action failed");
                }
            }
            Action::Hotkey(keys) => {
                info!(?source, action = "Hotkey", keys = ?keys, "action dispatch");
                let refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
                if let Err(e) = bitcat_core::hotkey::trigger_hotkey(&refs, 0.02) {
                    warn!(error = %e, "hotkey action failed");
                }
            }
            Action::ModifierTab(kind) => {
                // 按住态由 gamepad 物理层管理，Bus 只留日志（不做实际按键）。
                info!(?source, ?kind, action = "ModifierTab", "action dispatch");
            }
            Action::StartVoice => {
                // 语音录音条开启由 gamepad 物理层管理（HeldCombo 按住态）；
                // Bus 仅作为预留扩展点，后续如果加"前端/键盘触发一次按住态"再实现。
                info!(?source, action = "StartVoice", "action dispatch");
            }
            Action::Custom(tag) => {
                info!(?source, action = "Custom", tag = %tag, "action dispatch");
            }
        }
    }
}

// ---- 私有实现（把既有 cmd 的业务逻辑抽到 Bus） ----

/// Map an agent-facing game kind onto the existing app action variants.
pub(crate) fn action_for_start_game_kind(kind: bitcat_core::game_request::StartGameKind) -> Action {
    match kind {
        bitcat_core::game_request::StartGameKind::Snake => Action::PlayGameDefault,
        bitcat_core::game_request::StartGameKind::Memory => Action::PlayMemoryDefault,
        bitcat_core::game_request::StartGameKind::Catch => Action::PlayCatchDefault,
        bitcat_core::game_request::StartGameKind::Battle => Action::PlayBattleDefault,
        bitcat_core::game_request::StartGameKind::Gomoku => Action::PlayGomokuDefault,
    }
}

/// 将聊天文本写入 [`SharedPendingChat`]，由 [`chat_loop`](crate::gamepad::chat_loop) 消费。
fn submit_chat_impl(app: &AppHandle, text: String) {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        warn!("empty SubmitChat action skipped");
        return;
    }
    let state: tauri::State<'_, crate::gamepad::SharedPendingChat> = tauri::Manager::state(app);
    if let Err(e) = state.set(trimmed) {
        warn!(error = %e, "SharedPendingChat write failed");
    }
}

/// 打开对话输入：创建/获取 bubble 窗口，定位到宠物上方，并调用 JS 显示输入框。
///
/// `window.eval()` 调用有固有的脆弱性——WebView2 的 JS 上下文可能尚未就绪
/// （窗口刚创建、DOM 未完成渲染）。因此采用重试策略：最多尝试 10 次，每次间隔 30ms，
/// 直到 `__bubble_showInput` 函数存在并成功执行。超过重试次数后仅记录 warn 日志。
fn open_chat_impl(app: &AppHandle) {
    use tauri::Manager;

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => match crate::bubble::create_bubble_window(app) {
            Ok(w) => w,
            Err(e) => {
                warn!(error = %e, "create bubble window failed");
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
            debug!(attempt, "bubble input eval succeeded");
            return;
        }
    }
    warn!("bubble input eval failed after retries");
}

/// 播放指定名称的舞蹈动画，先校验舞蹈文件存在再发起播放请求。
fn play_dance_impl(dance_name: String) {
    if bitcat_core::dance::load_dance(&dance_name).is_err() {
        warn!(dance = %dance_name, "dance action target missing");
        return;
    }

    let req = bitcat_core::dance::PlayDanceRequest {
        name: dance_name,
        loops: Some(1),
        duration_ms: None,
    };
    if let Err(e) = bitcat_core::dance::request_play_dance(req) {
        warn!(error = %e, "request_play_dance failed");
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use bitcat_core::action::ActionDef;

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
            Action::PlayGameDefault,
            Action::PlayMemoryDefault,
            Action::PlayCatchDefault,
            Action::PlayBattleDefault,
            Action::PlayGomokuDefault,
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
    fn start_game_kind_maps_to_actions() {
        use bitcat_core::game_request::StartGameKind;

        assert_eq!(
            action_for_start_game_kind(StartGameKind::Snake),
            Action::PlayGameDefault
        );
        assert_eq!(
            action_for_start_game_kind(StartGameKind::Memory),
            Action::PlayMemoryDefault
        );
        assert_eq!(
            action_for_start_game_kind(StartGameKind::Catch),
            Action::PlayCatchDefault
        );
        assert_eq!(
            action_for_start_game_kind(StartGameKind::Battle),
            Action::PlayBattleDefault
        );
        assert_eq!(
            action_for_start_game_kind(StartGameKind::Gomoku),
            Action::PlayGomokuDefault
        );
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
    fn from_def_screenshot() {
        let d = def("screenshot");
        assert_eq!(ActionBus::from_def(&d), Some(Action::ScreenshotNow));
    }

    #[test]
    fn from_def_unknown_returns_none() {
        let d = def("mystery");
        assert!(ActionBus::from_def(&d).is_none());
    }
}
