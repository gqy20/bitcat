//! 手柄轮询、AI 对话循环与共享业务状态管理。
//!
//! 本模块是应用运行期的中枢：80ms 手柄轮询主循环（[`gamepad_loop`]）读取 SDL2 输入，
//! 独立的 [`chat_loop`] 消费前端提交的聊天消息并定时聚合长期记忆，
//! 两者通过 [`SharedChatCore`] 共享对话记忆、用户画像等业务状态。
//!
//! 设计上将手柄物理层（按钮检测、按住态）与 AI 对话链（上下文构建 → agent 调用 → 流式输出）
//! 解耦，确保无手柄或手柄断开时对话链仍可正常运行。
//! 对外通过 Tauri IPC 命令（`cmd_submit_chat` / `cmd_open_chat` 等）接收前端事件，
//! 对内通过 `pet-event` 通知前端宠物状态变化。

use crate::bubble;
use crate::commands::SharedWindowState;
use crate::game_input::{emit_game_input, GameInput};
use crate::joystick::{self, SdlGamepad};
use crate::panel;
use crate::pet_event_bus::SharedPetEventBus;
use crate::tts;
use crate::voice;
use ai_pad_core::action::{ActionConfig, ActionDef};
use ai_pad_core::agent::{AgentStreamEvent, PetAgent};
use ai_pad_core::agent_reaction::{extract_agent_reaction, fallback_agent_reaction};
use ai_pad_core::bridge::{handle_button_press, PetCommand};
use ai_pad_core::device::button_name;
use ai_pad_core::hotkey;
use ai_pad_core::logging::log_preview;
use ai_pad_core::memory::{LongTermMemory, MemoryStore, ProfileStore};
use ai_pad_core::pet_event::{
    agent_status_to_pet_event, tool_event_to_pet_event, PetEvent, PetMode, PetMood,
    PetNotificationKind,
};
use ai_pad_core::user_profile::UserProfile;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{debug, error, info, instrument, trace, warn};

// ========================================================================
// PetEvent：前端事件
// ========================================================================

/// 将桥层命令列表转换为前端事件列表，过滤掉不需要前端处理的命令。
pub fn commands_to_events(cmds: &[PetCommand]) -> Vec<PetEvent> {
    cmds.iter()
        .map(|cmd| match cmd {
            PetCommand::WalkTo { x } => PetEvent::walk_to(*x),
            PetCommand::ShowBubble { text } => PetEvent::show_bubble(text.clone()),
            PetCommand::Exit => PetEvent::exit(),
            PetCommand::PlayDance { name } => PetEvent::play_dance(name.clone()),
        })
        .collect()
}

/// 根据按钮索引生成宠物事件（状态切换 + 气泡）。
pub fn process_button(button_index: u32) -> Vec<PetEvent> {
    let (_agent_msg, pet_cmd) = handle_button_press(button_index, "");
    let mut events = Vec::new();
    match button_index {
        11 => events.push(PetEvent::ai_thinking()),
        10 => events.push(PetEvent::set_mode(PetMode::Sleep)),
        0 => events.push(PetEvent::react(PetMood::Happy)),
        _ => {}
    }
    if let Some(cmd) = pet_cmd {
        events.extend(commands_to_events(&[cmd]));
    }
    events
}

fn emit_pet_event(app: &AppHandle, event: PetEvent) {
    let bus: tauri::State<'_, SharedPetEventBus> = app.state();
    bus.emit(app, event);
}

// ========================================================================
// 聊天输入系统（前端提交 → chat_loop 消费）
// ========================================================================

/// 单槽消息队列：前端 `cmd_submit_chat` 写入，[`chat_loop`] 每 80ms 轮询消费。
///
/// 后写入的消息会覆盖先前的，确保只有最新的一条用户输入被发送给 AI。
pub struct SharedPendingChat {
    pending: Mutex<Option<String>>,
}

impl SharedPendingChat {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(None),
        }
    }

    /// 置入一条待消费的聊天文本（由 chat_loop 拉取）。
    pub fn set(&self, text: String) -> Result<(), String> {
        *self.pending.lock().map_err(|e| e.to_string())? = Some(text);
        Ok(())
    }
}

impl Default for SharedPendingChat {
    fn default() -> Self {
        Self::new()
    }
}

/// 前端触发的"提交聊天消息"命令，通过 ActionBus 写入 [`SharedPendingChat`]。
#[tauri::command]
pub async fn cmd_submit_chat(app: AppHandle, text: String) -> Result<(), String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err("消息不能为空".into());
    }
    crate::action_bus::ActionBus::dispatch(
        &app,
        crate::action_bus::Action::SubmitChat(trimmed),
        crate::action_bus::ActionSource::Frontend {
            cmd: "cmd_submit_chat".into(),
        },
    );
    Ok(())
}

/// 原子性地取出并清空待消费的聊天消息，返回 `None` 表示无新消息。
pub fn take_pending_chat(state: &State<'_, SharedPendingChat>) -> Option<String> {
    state.pending.lock().ok().and_then(|mut g| g.take())
}

// ========================================================================
// 共享业务状态：AI 对话 / 记忆 / 用户画像
// 从 gamepad_loop 解耦，使无手柄时对话链仍可运行
// ========================================================================

/// AI 对话链的共享业务状态，内含 5 个独立 Mutex。
///
/// 读写字段时各持短锁，**不要**同时持有两个以上的锁以避免死锁。
/// 当前所有访问点都遵循"获取 → 克隆/读取 → 立即释放"的模式。
///
/// # 线程模型
///
/// | 字段 | 写入线程 | 读取线程 |
/// |------|---------|---------|
/// | `memory` | chat_loop、gamepad_loop（run_ai_chat） | 同左 |
/// | `long_term` | chat_loop（run_ai_chat 写入 + 聚合标记） | gamepad_loop（run_ai_chat 读取） |
/// | `profile` | chat_loop（聚合更新） | gamepad_loop（run_ai_chat 读取） |
/// | `user_profile` | 仅初始化时写入（config/user.yml） | gamepad_loop、chat_loop |
/// | `last_aggregation` | chat_loop（聚合后更新） | chat_loop（定时检查） |
pub struct SharedChatCore {
    /// 短期对话记忆（滚动窗口），由 `chat_loop` 和 `gamepad_loop` 读写。
    pub memory: Mutex<MemoryStore>,
    /// 长期记忆条目，由 `chat_loop` 聚合并写入，`gamepad_loop` 检索。
    pub long_term: Mutex<LongTermMemory>,
    /// 自动聚合的用户画像，优先级低于 `user_profile`。
    pub profile: Mutex<ProfileStore>,
    /// 用户显式声明的身份信息（config/user.yml），为空时回退到 `profile`。
    pub user_profile: Mutex<UserProfile>,
    /// 上次画像聚合时间戳，`chat_loop` 用于判断是否触发定时聚合。
    pub last_aggregation: Mutex<std::time::Instant>,
}

impl SharedChatCore {
    pub fn new() -> Self {
        let memory = MemoryStore::load();
        let long_term = LongTermMemory::load();
        let profile = ProfileStore::load();
        let user_profile = UserProfile::load();
        info!(
            entries = memory.entries.len(),
            "[chat-core] 对话记忆系统已初始化"
        );
        info!(
            long_term = long_term.entries.len(),
            profile = !profile.profile_text.is_empty(),
            user_configured = !user_profile.is_empty(),
            "[chat-core] 长期记忆系统已初始化"
        );
        Self {
            memory: Mutex::new(memory),
            long_term: Mutex::new(long_term),
            profile: Mutex::new(profile),
            user_profile: Mutex::new(user_profile),
            last_aggregation: Mutex::new(std::time::Instant::now()),
        }
    }
}

impl Default for SharedChatCore {
    fn default() -> Self {
        Self::new()
    }
}

/// 延迟初始化的 AI Agent，基于 `OnceLock` 实现线程安全的一次性创建。
///
/// 任何线程首次调用 `get_or_init` 时触发初始化（读取 API key、构建 HTTP client）；
/// 初始化失败则记录 `None`，后续调用直接返回 `None`（对话不可用）。
pub struct SharedAgent {
    inner: std::sync::OnceLock<Option<PetAgent>>,
}

impl SharedAgent {
    pub fn new() -> Self {
        Self {
            inner: std::sync::OnceLock::new(),
        }
    }

    pub fn get_or_init(&self) -> Option<&PetAgent> {
        self.inner
            .get_or_init(|| match PetAgent::new() {
                Ok(a) => {
                    info!("AI Agent 初始化成功 (8Bit Cat)");
                    Some(a)
                }
                Err(e) => {
                    error!(error = %e, "AI Agent 初始化失败，后续对话将不可用");
                    None
                }
            })
            .as_ref()
    }
}

impl Default for SharedAgent {
    fn default() -> Self {
        Self::new()
    }
}

/// 前端调试日志桥接：将前端的 console 输出转发到 Rust 日志系统。
#[tauri::command]
pub async fn cmd_pet_log(msg: String) -> Result<(), String> {
    let preview = log_preview(&msg, 80);
    info!(
        msg_chars = msg.chars().count(),
        msg_preview = %preview,
        "pet frontend log"
    );
    Ok(())
}

/// 前端触发的"退出对话"命令，通过 ActionBus 统一调度。
#[tauri::command]
pub async fn cmd_exit_chat(app: AppHandle) -> Result<(), String> {
    crate::action_bus::ActionBus::dispatch(
        &app,
        crate::action_bus::Action::ExitChat,
        crate::action_bus::ActionSource::Frontend {
            cmd: "cmd_exit_chat".into(),
        },
    );
    Ok(())
}

/// 轻量"进入 chat"命令：只置位 chat_active=true，不负责开窗口/展示 UI。
///
/// 与 cmd_open_chat 的区别：
/// - cmd_open_chat 走"点击嘴巴"路径，会创建窗口 + eval showInput
/// - cmd_enter_chat 给前端用：无论通过哪条路径展开了输入框（bubble-end 自动展开、
///   稳定检测兜底、双击气泡、用户点击 input），都通知后端立刻锁住截图 / Vision。
#[tauri::command]
pub async fn cmd_enter_chat(app: AppHandle) -> Result<(), String> {
    let state: State<bubble::SharedBubble> = app.state();
    let was_active = state.is_chat_active();
    state.set_chat_active(true);
    if !was_active {
        info!("[cmd_enter_chat] chat 模式开启（截图已锁定）");
    }
    Ok(())
}

/// 前端触发的"打开对话"命令，通过 ActionBus 创建/定位 bubble 窗口并显示输入框。
#[tauri::command]
pub async fn cmd_open_chat(app: AppHandle) -> Result<(), String> {
    crate::action_bus::ActionBus::dispatch(
        &app,
        crate::action_bus::Action::OpenChat,
        crate::action_bus::ActionSource::Frontend {
            cmd: "cmd_open_chat".into(),
        },
    );
    Ok(())
}

// ========================================================================
// 手柄选择 + 主循环 + AI 对话 + 动作执行
// ========================================================================

/// 从 SDL2 枚举到的设备列表中选出一个真正的游戏手柄。
///
/// 依次按优先级筛选：排除键鼠接收器等伪设备 → 优先匹配已知手柄名称
/// （Xbox / DualSense / 8BitDo 等）→ 按帽子/轴数量兜底 → 取过滤后第一个。
/// 全部不满足时返回 `None`，主循环会在下一秒重试。
fn choose_gamepad(pads: &[joystick::GamepadInfo]) -> Option<&joystick::GamepadInfo> {
    let is_kbm_like = |name: &str| {
        let n = name.to_lowercase();
        ["link-km", "receiver", "keyboard", "mouse", "wireless link"]
            .iter()
            .any(|kw| n.contains(kw))
    };
    let is_preferred = |name: &str| {
        let n = name.to_lowercase();
        [
            "controller",
            "gamepad",
            "8bitdo",
            "xbox",
            "dualshock",
            "dualsense",
            "joy-con",
            "joycon",
            "pro controller",
        ]
        .iter()
        .any(|kw| n.contains(kw))
    };

    let filtered: Vec<&joystick::GamepadInfo> =
        pads.iter().filter(|p| !is_kbm_like(&p.name)).collect();

    if let Some(p) = filtered.iter().find(|p| is_preferred(&p.name)) {
        return Some(*p);
    }
    if let Some(p) = filtered.iter().find(|p| p.num_hats >= 1 || p.num_axes >= 2) {
        return Some(*p);
    }
    filtered.first().copied()
}

/// 手柄轮询主循环，80ms tick。
///
/// 外层循环枚举 SDL2 设备并通过 [`choose_gamepad`] 筛选手柄；内层循环读取按钮/帽子状态，
/// 经 bridge 映射为宠物事件和 AI 对话触发，同时处理面板导航、语音按住态、热键动作等。
/// 手柄断开后自动回到外层重新枚举，不会退出线程。
#[instrument(skip(app))]
pub fn gamepad_loop(app: &tauri::AppHandle) {
    debug!("[gamepad] gamepad_loop 开始");
    let sdl = match SdlGamepad::init() {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "SDL2 初始化失败");
            return;
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Tokio 运行时创建失败");
            return;
        }
    };

    let mut action_config = ActionConfig::load("config/actions.yml").ok();
    let ac = action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0);
    info!(action_count = ac, "已加载 {ac} 个动作绑定");

    let memory_config = ai_pad_core::prompts::PromptsConfig::load().memory;

    let mut alt_tab = HeldModifier::new(0x12);
    let mut ctrl_tab = HeldModifier::new(0x11);
    let mut held_voice = HeldCombo::new();

    let mut last_warn: Option<std::time::Instant> = None;
    loop {
        if crate::shutdown::is_requested() {
            info!("[gamepad_loop] shutdown requested, exiting");
            break;
        }
        let pads = match SdlGamepad::list_gamepads(&sdl) {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "枚举手柄失败，2s 后重试");
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
        };

        for p in &pads {
            debug!(index = p.index, name = %p.name, buttons = p.num_buttons, hats = p.num_hats, axes = p.num_axes, "候选设备");
        }

        let target = match choose_gamepad(&pads) {
            Some(t) => t.clone(),
            None => {
                if last_warn.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(10)) {
                    warn!(
                        enumerated = pads.len(),
                        "未检测到真正的游戏手柄（已自动跳过键鼠接收器等），每秒重试中..."
                    );
                    last_warn = Some(std::time::Instant::now());
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };
        last_warn = None;

        info!(
            index = target.index,
            name = %target.name,
            buttons = target.num_buttons,
            hats = target.num_hats,
            axes = target.num_axes,
            "✓ 选中手柄 [{}] {}",
            target.index,
            target.name
        );

        let mut gamepad = match SdlGamepad::open(&sdl, target.index) {
            Ok(g) => g,
            Err(e) => {
                error!(error = %e, "打开手柄失败，1s 后重试");
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };
        info!("ai-pad 启动（手柄已就绪）");

        let mut prev_buttons: u32 = 0;
        let mut prev_hat: Option<(i32, i32)> = None;
        let mut attach_check_tick: u32 = 0;

        loop {
            // 配置热重载
            {
                if crate::shutdown::is_requested() {
                    alt_tab.release();
                    ctrl_tab.release();
                    held_voice.release_keys();
                    info!("[gamepad_loop] shutdown requested, releasing held keys");
                    return;
                }
                let ws: tauri::State<'_, SharedWindowState> = app.state();
                if ws.config_reload.load(Ordering::SeqCst) {
                    ws.config_reload.store(false, Ordering::SeqCst);
                    action_config = ActionConfig::load("config/actions.yml").ok();
                    info!(
                        actions = action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0),
                        "gamepad_loop 配置已刷新"
                    );
                }
            }

            attach_check_tick += 1;
            if attach_check_tick >= 12 {
                attach_check_tick = 0;
                if !gamepad.is_attached() {
                    alt_tab.release();
                    ctrl_tab.release();
                    held_voice.release_keys();
                    warn!(index = target.index, name = %target.name, "手柄已断开，返回外层重新枚举");
                    break;
                }
            }

            let panel_visible = app
                .get_webview_window("panel")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            let game_active = crate::game::is_game_active(app);

            let buttons = gamepad.read_buttons();
            let new_presses = (buttons ^ prev_buttons) & buttons;

            if new_presses != 0 {
                for bit in 0..32 {
                    if new_presses & (1 << bit) != 0 {
                        let idx = bit as u32;
                        let name = button_name(idx as usize).unwrap_or("?");
                        debug!(button_idx = idx, button_name = name, "按下 #{idx} {name}");

                        if (alt_tab.held && name != "L1") || (ctrl_tab.held && name != "L2") {
                            alt_tab.release();
                            ctrl_tab.release();
                        }

                        if game_active {
                            let is_battle = matches!(
                                crate::game::current_game_type(app),
                                Some(ai_pad_core::minigame::MinigameType::Battle)
                            );
                            if is_battle {
                                match name {
                                    "A" => {
                                        info!("→ 战斗普通攻击");
                                        emit_game_input(app, GameInput::AttackPrimary);
                                    }
                                    "B" => {
                                        info!("→ 游戏取消");
                                        emit_game_input(app, GameInput::Cancel);
                                    }
                                    "X" => {
                                        info!("→ 战斗技能 1");
                                        emit_game_input(app, GameInput::Skill { slot: 1 });
                                    }
                                    "Y" => {
                                        info!("→ 战斗技能 2");
                                        emit_game_input(app, GameInput::Skill { slot: 2 });
                                    }
                                    "L1" => {
                                        info!("→ 战斗防御");
                                        emit_game_input(app, GameInput::Guard);
                                    }
                                    "R1" => {
                                        info!("→ 战斗技能 3");
                                        emit_game_input(app, GameInput::Skill { slot: 3 });
                                    }
                                    "Start" => {
                                        info!("→ 游戏暂停");
                                        emit_game_input(app, GameInput::Pause);
                                    }
                                    _ => {}
                                }
                            } else {
                                match name {
                                    "A" => {
                                        info!("→ 游戏确认");
                                        emit_game_input(app, GameInput::Confirm);
                                    }
                                    "B" => {
                                        info!("→ 游戏取消");
                                        emit_game_input(app, GameInput::Cancel);
                                    }
                                    "Start" => {
                                        info!("→ 游戏暂停");
                                        emit_game_input(app, GameInput::Pause);
                                    }
                                    _ => {}
                                }
                            }
                            continue;
                        }

                        if name == "Home" {
                            info!("→ 切换面板");
                            panel::toggle_panel(app);
                            continue;
                        }

                        if panel_visible {
                            match name {
                                "A" => {
                                    info!("→ 面板确认");
                                    let _ = app.emit("panel-confirm", ());
                                }
                                "B" => {
                                    info!("→ 面板关闭");
                                    let _ = app.emit("panel-close", ());
                                }
                                _ => {}
                            }
                            continue;
                        }

                        let (agent_msg, pet_cmd) = handle_button_press(idx, "");

                        // 舞蹈命令：走 bridge 统一播放管线，启用 is_dancing 状态
                        if let Some(PetCommand::PlayDance { name }) = &pet_cmd {
                            info!(dance = %name, "[gamepad] Y 键 → 播放舞蹈");
                            if ai_pad_core::dance::load_dance(name).is_err() {
                                warn!(dance = %name, "[gamepad] 舞蹈不存在，无法播放");
                            } else {
                                let req = ai_pad_core::dance::PlayDanceRequest {
                                    name: name.clone(),
                                    loops: Some(1), // Y 键默认单次
                                    duration_ms: None,
                                };
                                if let Err(e) = ai_pad_core::dance::request_play_dance(req) {
                                    warn!(error = %e, "手柄触发舞蹈失败");
                                }
                            }
                        }

                        let events = process_button(idx);
                        for evt in events {
                            emit_pet_event(app, evt);
                        }

                        if let Some(msg) = &agent_msg {
                            let agent_state: State<SharedAgent> = app.state();
                            if let Some(ag) = agent_state.get_or_init() {
                                let core: State<SharedChatCore> = app.state();
                                let preview = log_preview(msg, 60);
                                info!(
                                    msg_chars = msg.chars().count(),
                                    msg_preview = %preview,
                                    "gamepad chat requested"
                                );
                                run_ai_chat(&rt, ag, app, msg, "", &core, &memory_config);
                            }
                        }

                        if let Some(ref config) = action_config {
                            if let Some(action_def) = config.actions.get(name) {
                                info!(name = name, action_type = %action_def.action_type, "→ {} ({})", name, action_def.action_type);
                                if let Some(action) =
                                    crate::action_bus::ActionBus::from_def(action_def)
                                {
                                    // ModifierTab 按住态需要 gamepad 物理层直接维护
                                    // HeldModifier；Bus 只发日志，真正按键仍走 execute_action。
                                    if matches!(action, crate::action_bus::Action::ModifierTab(_)) {
                                        execute_action(
                                            action_def,
                                            &config.defaults,
                                            &mut alt_tab,
                                            &mut ctrl_tab,
                                        );
                                    } else {
                                        crate::action_bus::ActionBus::dispatch(
                                            app,
                                            action,
                                            crate::action_bus::ActionSource::Gamepad {
                                                button: name.to_string(),
                                            },
                                        );
                                    }
                                } else {
                                    // from_def 返回 None（如 voice 或未知类型）→ 走原 execute_action 兜底
                                    execute_action(
                                        action_def,
                                        &config.defaults,
                                        &mut alt_tab,
                                        &mut ctrl_tab,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Voice 按住检测
            let mut voice_just_released = false;
            let mut voice_just_pressed = false;
            if game_active {
                if held_voice.is_held() {
                    held_voice.cancel();
                    debug!("[voice] 游戏运行中，取消语音按住态");
                }
            } else if let Some(ref config) = action_config {
                let mut voice_bits: u32 = 0;
                for (name, action_def) in &config.actions {
                    if action_def.action_type == "voice" {
                        if let Some(bit) = name_to_bit(name) {
                            voice_bits |= 1 << bit;
                        }
                    }
                }
                let voice_active = (buttons & voice_bits) != 0;
                let (jp, jr) = held_voice.detect(voice_active);
                voice_just_pressed = jp;
                voice_just_released = jr;
            }

            if voice_just_pressed {
                match voice::open_voice_capture(app) {
                    Ok(()) => info!("[voice] 录音条已显示并强制前台化"),
                    Err(e) => warn!(error = %e, "[voice] 打开录音条失败"),
                }
                std::thread::sleep(std::time::Duration::from_millis(80));
                if let Some(ref config) = action_config {
                    held_voice.press_keys(config);
                }
            }

            if voice_just_released {
                held_voice.release_keys();
                info!("[voice] 等待识别注入完成 (700ms)...");
                std::thread::sleep(std::time::Duration::from_millis(700));
                match voice::take_voice_text(app) {
                    Ok(raw) => {
                        let text = raw.trim().to_string();
                        if text.is_empty() {
                            warn!("[voice] 虚拟输入框为空 (识别可能失败或焦点被抢走)");
                        } else {
                            let preview = log_preview(&text, 60);
                            info!(
                                voice_chars = text.chars().count(),
                                voice_preview = %preview,
                                "[voice] 识别完成"
                            );
                            let agent_state: State<SharedAgent> = app.state();
                            if let Some(ag) = agent_state.get_or_init() {
                                let core: State<SharedChatCore> = app.state();
                                run_ai_chat(&rt, ag, app, &text, "[voice]", &core, &memory_config);
                            } else {
                                warn!("[voice] AI Agent 未初始化");
                            }
                        }
                    }
                    Err(e) => warn!(error = %e, "[voice] 读取虚拟输入框失败"),
                }
            }

            prev_buttons = buttons;

            // 注意：Bubble 聊天输入消费 + 长期记忆聚合 已迁移到 chat_loop
            // 这里只处理手柄原生事件

            // 方向键
            let hat = gamepad.read_hat(0);
            if game_active {
                if hat != prev_hat {
                    if let Some((dx, dy)) = hat {
                        info!(dx = dx, dy = dy, "→ 游戏方向");
                        emit_game_input(app, GameInput::Direction { dx, dy: -dy });
                    }
                }
            } else if panel_visible {
                if hat != prev_hat {
                    if let Some((dx, dy)) = hat {
                        info!(dx = dx, dy = dy, "→ 面板导航");
                        let _ = app.emit("panel-nav", (dx, dy));
                    }
                }
            } else if let Some((dx, dy)) = hat {
                alt_tab.release();
                ctrl_tab.release();
                let speed = 3;
                if dy > 0 {
                    let _ = hotkey::send_scroll(120 * speed);
                } else if dy < 0 {
                    let _ = hotkey::send_scroll(-120 * speed);
                }
                if dx > 0 {
                    let _ = hotkey::send_scroll_h(120 * speed);
                } else if dx < 0 {
                    let _ = hotkey::send_scroll_h(-120 * speed);
                }
            }
            prev_hat = hat;

            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// 独立业务循环：消费 bubble 输入 + 定时聚合长期记忆
///
/// 与 gamepad_loop **平级独立运行**。没有手柄、手柄断开、手柄未识别时，
/// 本循环依然按常规节奏处理前端 `cmd_submit_chat` 提交的消息以及记忆聚合。
#[instrument(skip(app))]
pub fn chat_loop(app: &tauri::AppHandle) {
    info!("[chat_loop] 已启动（独立于手柄）");

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "[chat_loop] Tokio 运行时创建失败");
            return;
        }
    };

    let memory_config = ai_pad_core::prompts::PromptsConfig::load().memory;
    let prompts_cfg = ai_pad_core::prompts::PromptsConfig::load();

    loop {
        // --- 1. 消费 bubble 聊天输入 ---
        if crate::shutdown::is_requested() {
            info!("[chat_loop] shutdown requested, exiting");
            break;
        }
        let chat_msg = {
            let pc: State<SharedPendingChat> = app.state();
            take_pending_chat(&pc)
        };
        if let Some(msg) = chat_msg {
            let agent_state: State<SharedAgent> = app.state();
            if let Some(ag) = agent_state.get_or_init() {
                let core: State<SharedChatCore> = app.state();
                let preview = log_preview(&msg, 60);
                info!(
                    msg_chars = msg.chars().count(),
                    msg_preview = %preview,
                    "[chat] bubble input received"
                );
                run_ai_chat(&rt, ag, app, &msg, "[chat]", &core, &memory_config);
            } else {
                let preview = log_preview(&msg, 60);
                warn!(
                    msg_chars = msg.chars().count(),
                    msg_preview = %preview,
                    "[chat] AI Agent 未就绪，消息被丢弃"
                );
            }
        }

        // --- 2. 定时聚合长期记忆 → 用户画像 ---
        {
            let core: State<SharedChatCore> = app.state();
            let agg_interval = std::time::Duration::from_secs(
                (prompts_cfg.memory_v2.aggregation_interval_min as u64) * 60,
            );
            let snapshot = {
                let lt = match core.long_term.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        continue;
                    }
                };
                let pf = match core.profile.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        continue;
                    }
                };
                let la = match core.last_aggregation.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        continue;
                    }
                };
                (
                    lt.unaggregated_entries().len(),
                    pf.profile_text.is_empty(),
                    la.elapsed(),
                )
            };
            let (unagg_count, profile_empty, elapsed) = snapshot;
            let should_aggregate = unagg_count >= 20 || (!profile_empty && elapsed >= agg_interval);

            if should_aggregate && unagg_count > 0 {
                match ai_pad_core::ai_config::AiConfig::load() {
                    Ok(cfg) => {
                        // 快照条目和现有画像（持锁克隆一次，聚合 IO 不持锁）
                        let (entries_cloned, cur_profile) = {
                            let lt = core.long_term.lock().unwrap();
                            let pf = core.profile.lock().unwrap();
                            let entries: Vec<ai_pad_core::memory::LongTermEntry> =
                                lt.unaggregated_entries().into_iter().cloned().collect();
                            (entries, pf.profile_text.clone())
                        };
                        info!(
                            count = unagg_count,
                            "[chat_loop] 开始聚合长期记忆 → 用户画像"
                        );
                        let entry_refs: Vec<&ai_pad_core::memory::LongTermEntry> =
                            entries_cloned.iter().collect();
                        let agg_prompt = ai_pad_core::prompts::PromptsConfig::default()
                            .aggregation
                            .prompt;
                        let agg_result = rt.block_on(ai_pad_core::memory::aggregate_profile(
                            &entry_refs,
                            &cur_profile,
                            &cfg,
                            &agg_prompt,
                        ));
                        match agg_result {
                            Ok(new_profile) => {
                                if let Ok(mut pf) = core.profile.lock() {
                                    pf.update(&new_profile);
                                    let _ = pf.save();
                                }
                                if let Ok(mut lt) = core.long_term.lock() {
                                    lt.mark_all_aggregated();
                                    let _ = lt.save();
                                }
                                if let Ok(mut la) = core.last_aggregation.lock() {
                                    *la = std::time::Instant::now();
                                }
                                let new_len = core
                                    .profile
                                    .lock()
                                    .map(|p| p.profile_text.chars().count())
                                    .unwrap_or(0);
                                info!(profile_len = new_len, "[chat_loop] 用户画像已更新");
                            }
                            Err(e) => {
                                warn!(error = %e, "[chat_loop] 记忆聚合失败，下次重试")
                            }
                        }
                    }
                    Err(e) => warn!(error = %e, "[chat_loop] AI 配置加载失败，跳过聚合"),
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

/// RAII 守卫：创建时将 `chat_active` 置为 `true`，`Drop` 时自动还原为 `false`。
///
/// **设计意图**：AI 对话期间截屏线程应跳过 Vision 分析（避免并发 token 消耗和
/// 内容冲突）。无论 `run_ai_chat` 通过正常返回、`?` 提前退出还是 panic 退出，
/// 守卫的 `Drop` 都会执行，保证截图线程在下一轮恢复工作。
struct ChatActiveGuard {
    app: tauri::AppHandle,
    log_prefix: String,
}

impl ChatActiveGuard {
    /// 创建守卫并立即将 `chat_active` 置为 `true`，锁定截图线程。
    fn new(app: &tauri::AppHandle, log_prefix: &str) -> Self {
        let bubble_state: tauri::State<'_, bubble::SharedBubble> = app.state();
        bubble_state.set_chat_active(true);
        info!("{log_prefix}[chat_guard] chat_active=true (截屏已锁定)");
        Self {
            app: app.clone(),
            log_prefix: log_prefix.to_string(),
        }
    }
}

impl Drop for ChatActiveGuard {
    fn drop(&mut self) {
        let bubble_state: tauri::State<'_, bubble::SharedBubble> = self.app.state();
        bubble_state.set_chat_active(false);
        info!(
            "{prefix}[chat_guard] chat_active=false (截屏已解锁)",
            prefix = self.log_prefix
        );
    }
}

/// 统一的 AI 流式对话执行（线程安全版）
///
/// 锁策略：
/// - 读取上下文时各持短锁，读完立即释放
/// - 流式网络 IO 期间 **完全不持锁**，不阻塞其他线程
/// - 写入记忆时再次短锁
///
/// 截屏互斥：函数入口 set chat_active=true，RAII guard 保证 panic/return 都会 false。
pub fn run_ai_chat(
    rt: &tokio::runtime::Runtime,
    agent: &PetAgent,
    app: &tauri::AppHandle,
    msg: &str,
    log_prefix: &str,
    core: &SharedChatCore,
    memory_config: &ai_pad_core::memory::MemoryConfig,
) {
    let tag = if log_prefix.is_empty() { "" } else { " " };
    let msg_preview = log_preview(msg, 60);
    info!(
        model = %agent.config.model,
        msg_chars = msg.chars().count(),
        msg_preview = %msg_preview,
        "{log_prefix}AI chat started"
    );

    // RAII 锁：整个 chat 期间阻止截屏线程进入 Vision 分析；panic 或 early return 时自动释放
    let _chat_guard = ChatActiveGuard::new(app, log_prefix);

    if let Err(e) = bubble::start_streaming_bubble(app) {
        warn!(error = %e, "{log_prefix}气泡启动错误");
        return;
    }

    // ---- 构建上下文：各字段单独短锁 ----
    let ctx = match core.memory.lock() {
        Ok(g) => g.build_context(memory_config),
        Err(e) => {
            warn!(error = %e, "memory 锁中毒，跳过上下文");
            return;
        }
    };

    // 用户显式声明优先（config/user.yml），为空时回退到自动聚合画像
    let user_profile_ctx = match core.user_profile.lock() {
        Ok(up) => up.build_context(),
        Err(e) => {
            warn!(error = %e, "user_profile 锁中毒，跳过用户配置");
            String::new()
        }
    };
    let profile_ctx = if user_profile_ctx.is_empty() {
        match core.profile.lock() {
            Ok(g) => g.build_context(),
            Err(e) => {
                warn!(error = %e, "profile 锁中毒，跳过上下文");
                String::new()
            }
        }
    } else {
        String::new()
    };
    let long_term_ctx = match core.long_term.lock() {
        Ok(g) => g.retrieve(msg, 800),
        Err(e) => {
            warn!(error = %e, "long_term 锁中毒，跳过上下文");
            return;
        }
    };
    let summary_store = ai_pad_core::screen_summary::ScreenSummaryStore::load();
    let summary_config = ai_pad_core::prompts::PromptsConfig::load().screen_summary;
    let summary_ctx = summary_store.build_context(&summary_config);
    let recent_ctx = ai_pad_core::screenshot::build_recent_analyses_context(10, 1500);
    let context_parts: Vec<&str> = [
        &user_profile_ctx,
        &profile_ctx,
        &long_term_ctx,
        &ctx,
        &recent_ctx,
        &summary_ctx,
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .map(|s| s.as_str())
    .collect();
    let enriched_msg = if context_parts.is_empty() {
        msg.to_string()
    } else {
        format!("{}\n用户说: {msg}", context_parts.join("\n"))
    };
    debug!(
        user_profile_ctx_chars = user_profile_ctx.chars().count(),
        profile_ctx_chars = profile_ctx.chars().count(),
        long_term_ctx_chars = long_term_ctx.chars().count(),
        memory_ctx_chars = ctx.chars().count(),
        recent_ctx_chars = recent_ctx.chars().count(),
        summary_ctx_chars = summary_ctx.chars().count(),
        enriched_msg_chars = enriched_msg.chars().count(),
        "{log_prefix}chat context assembled"
    );

    // ---- 流式 IO：不持锁 ----
    let app_for_chunks = app.clone();
    let prefix = log_prefix.to_string();
    let prefix_for_log = prefix.clone();
    let tool_summaries = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let tool_summaries_for_stream = tool_summaries.clone();
    emit_pet_event(app, PetEvent::ai_thinking());
    let stream_result = rt.block_on(agent.chat_stream(&enriched_msg, move |event| match event {
        AgentStreamEvent::Text { text } => {
            trace!(
                chunk_chars = text.chars().count(),
                "{prefix_for_log}{tag}AI chunk"
            );
            let _ = bubble::append_bubble_chunk(&app_for_chunks, &text);
        }
        AgentStreamEvent::Status { status } => {
            debug!(status = ?status, "{prefix_for_log}{tag}AI stream status");
            emit_pet_event(&app_for_chunks, agent_status_to_pet_event(status));
        }
        AgentStreamEvent::Tool { event } => {
            debug!(
                tool = %event.tool_name,
                phase = ?event.phase,
                "{prefix_for_log}{tag}AI tool event"
            );
            if let Some(pet_event) = tool_event_to_pet_event(&event) {
                emit_pet_event(&app_for_chunks, pet_event);
            }
            if let Ok(mut summaries) = tool_summaries_for_stream.lock() {
                let preview = event.result_preview.as_deref().unwrap_or("");
                summaries.push(format!(
                    "{}:{} success={:?} elapsed={:?} {}",
                    event.tool_name,
                    event.phase.as_str(),
                    event.success,
                    event.elapsed_ms,
                    preview
                ));
            }
            let _ = bubble::emit_tool_event(
                &app_for_chunks,
                bubble::BubbleToolPayload {
                    tool_name: event.tool_name,
                    label: event.label,
                    kind: event.kind.as_str().to_string(),
                    phase: event.phase.as_str().to_string(),
                    call_id: event.call_id,
                    internal_call_id: event.internal_call_id,
                    result_preview: event.result_preview,
                    success: event.success,
                    elapsed_ms: event.elapsed_ms,
                },
            );
        }
    }));
    let _ = bubble::finalize_bubble(app);
    emit_pet_event(
        app,
        PetEvent::ClearNotification {
            kind: Some(PetNotificationKind::AiThinking),
        },
    );
    emit_pet_event(
        app,
        PetEvent::ClearNotification {
            kind: Some(PetNotificationKind::AiWriting),
        },
    );
    emit_pet_event(
        app,
        PetEvent::ClearNotification {
            kind: Some(PetNotificationKind::ToolPreparing),
        },
    );
    emit_pet_event(
        app,
        PetEvent::ClearNotification {
            kind: Some(PetNotificationKind::ToolRunning),
        },
    );

    match stream_result {
        Ok(reply) => {
            // 短期记忆：短锁写入
            if let Ok(mut memory) = core.memory.lock() {
                memory.record_conversation(msg, &reply, memory_config);
                if let Err(e) = memory.save() {
                    warn!(error = %e, "保存对话记忆失败");
                }
            } else {
                warn!("memory 锁中毒，跳过短期记忆写入");
            }

            let reply_preview = log_preview(&reply, 80);
            info!(
                model = %agent.config.model,
                reply_chars = reply.chars().count(),
                reply_preview = %reply_preview,
                "{prefix}AI chat completed"
            );
            let reply_for_tts = reply.clone();
            let tts_on = ai_pad_core::app_settings::AppSettings::load()
                .appearance
                .tts_enabled;
            if tts_on {
                std::thread::spawn(move || {
                    tts::speak(&reply_for_tts);
                });
            }

            let summaries = tool_summaries.lock().map(|g| g.clone()).unwrap_or_default();
            let reaction_result = rt.block_on(tokio::time::timeout(
                std::time::Duration::from_secs(8),
                extract_agent_reaction(&agent.config, msg, &reply, &summaries),
            ));
            let reaction = match reaction_result {
                Ok(Ok(reaction)) => reaction,
                Ok(Err(e)) => fallback_agent_reaction(&reply, &e),
                Err(_) => fallback_agent_reaction(&reply, "AgentReaction timed out"),
            };
            let speech = if reaction.speech.is_empty() {
                None
            } else {
                Some(reaction.speech.clone())
            };
            emit_pet_event(
                app,
                PetEvent::React {
                    mood: reaction.mood,
                    speech,
                    ttl_ms: None,
                },
            );

            if !reaction.memory_candidates.is_empty() {
                if let Ok(mut long_term) = core.long_term.lock() {
                    for candidate in &reaction.memory_candidates {
                        long_term.record_candidate(candidate, msg, &reply, 200);
                    }
                    if let Err(e) = long_term.save() {
                        warn!(error = %e, "保存长期记忆候选失败");
                    }
                } else {
                    warn!("long_term 锁中毒，跳过长期记忆候选写入");
                }
            }
        }
        Err(e) => {
            emit_pet_event(
                app,
                PetEvent::Notify {
                    kind: PetNotificationKind::ToolFailed,
                    body: Some(format!("AI 对话失败: {e}")),
                    ttl_ms: Some(15_000),
                    refresh: true,
                },
            );
            warn!(model = %agent.config.model, error = %e, "{log_prefix} AI 错误");
        }
    }
}

/// 执行原始动作定义（未迁移到 ActionBus 的遗留路径）。
///
/// 处理 launch / script / hotkey / voice 四种类型。`ModifierTab` 按住态的按键
/// 状态由 `alt_tab` / `ctrl_tab` 参数维护，不经过 ActionBus。
fn execute_action(
    action: &ActionDef,
    defaults: &ai_pad_core::action::Defaults,
    alt_tab: &mut HeldModifier,
    ctrl_tab: &mut HeldModifier,
) {
    match action.action_type.as_str() {
        "launch" => {
            let program = match &action.program {
                Some(p) => p.as_str(),
                None => return,
            };
            let args = action.args.as_deref().unwrap_or("");
            let _ = ai_pad_core::action::launch_program(
                program,
                args,
                &action.workdir,
                action.terminal,
                &defaults.terminal,
            );
        }
        "voice" => {}
        "script" => {
            if let Some(cmd) = &action.command {
                let _ = std::process::Command::new("powershell")
                    .args(["-Command", cmd])
                    .spawn();
            }
        }
        "hotkey" => {
            if let Some(trigger) = &action.trigger {
                let has_alt = trigger.iter().any(|k| k.to_lowercase() == "alt");
                let has_ctrl = trigger.iter().any(|k| k.to_lowercase() == "ctrl");
                let has_tab = trigger.iter().any(|k| k.to_lowercase() == "tab");

                if has_alt && has_tab {
                    alt_tab.press();
                } else if has_ctrl && has_tab {
                    ctrl_tab.press();
                } else {
                    let key_refs: Vec<&str> = trigger.iter().map(|s| s.as_str()).collect();
                    if let Err(e) = hotkey::trigger_hotkey(&key_refs, 0.02) {
                        warn!(error = %e, "热键触发失败");
                    }
                }
            }
        }
        other => {
            warn!(action_type = other, "未知动作类型");
        }
    }
}

// ---- 辅助结构体 ----

/// Alt / Ctrl 等修饰键的按住态管理器。
///
/// 首次 `press()` 发送 key_down 并标记 held；后续 `press()` 只发送 Tab 按键。
/// `release()` 仅在 held 时发送 key_up，方向键按下时也会强制释放。
pub(crate) struct HeldModifier {
    vk: u16,
    held: bool,
}

impl HeldModifier {
    fn new(vk: u16) -> Self {
        Self { vk, held: false }
    }
    fn press(&mut self) {
        if !self.held {
            let _ = hotkey::key_down(self.vk);
            self.held = true;
        }
        let _ = hotkey::key_down(0x09);
        let _ = hotkey::key_up(0x09);
    }
    fn release(&mut self) {
        if self.held {
            let _ = hotkey::key_up(self.vk);
            self.held = false;
        }
    }
}

/// 输入法语音热键组合的按住态管理器。
///
/// 按下时发送配置的虚拟按键组合（激活输入法语音模式），松开时逆序释放。
/// `detect()` 方法根据按钮状态变化返回 `(just_pressed, just_released)` 元组。
pub struct HeldCombo {
    vks: Vec<u16>,
    held: bool,
}

impl Default for HeldCombo {
    fn default() -> Self {
        Self::new()
    }
}

impl HeldCombo {
    pub fn new() -> Self {
        Self {
            vks: Vec::new(),
            held: false,
        }
    }

    pub fn detect(&mut self, active: bool) -> (bool, bool) {
        match (active, self.held) {
            (true, false) => {
                self.held = true;
                (true, false)
            }
            (false, true) => {
                self.held = false;
                (false, true)
            }
            _ => (false, false),
        }
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub fn press_keys(&mut self, config: &ai_pad_core::action::ActionConfig) {
        let mut vks = Vec::new();
        for action_def in config.actions.values() {
            if action_def.action_type == "voice" {
                if let Some(voice) = &action_def.voice {
                    let keys: Vec<&str> = voice.trigger.iter().map(|s| s.as_str()).collect();
                    vks.extend(hotkey::parse_keys(&keys));
                }
            }
        }
        self.vks = vks;
        for &vk in &self.vks {
            let _ = hotkey::key_down(vk);
        }
        info!(vk_count = self.vks.len(), "→ 输入法语音热键已按下");
    }

    pub fn release_keys(&mut self) {
        for &vk in self.vks.iter().rev() {
            let _ = hotkey::key_up(vk);
        }
        info!("→ 输入法语音热键已松开");
    }

    pub fn cancel(&mut self) {
        self.release_keys();
        self.held = false;
        self.vks.clear();
    }
}

/// 将按钮名称（"A" / "B" / "L1" 等）映射到 SDL2 按钮位索引。
fn name_to_bit(name: &str) -> Option<u32> {
    match name {
        "A" => Some(0),
        "B" => Some(1),
        "X" => Some(3),
        "Y" => Some(4),
        "L1" => Some(6),
        "R1" => Some(7),
        "L2" => Some(8),
        "R2" => Some(9),
        "Select" => Some(10),
        "Start" => Some(11),
        "Home" => Some(12),
        _ => None,
    }
}

// ========================================================================
// 测试
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_notify() {
        let e = PetEvent::ai_thinking();
        assert!(matches!(e, PetEvent::Notify { .. }));
    }

    #[test]
    fn test_event_bubble() {
        let e = PetEvent::show_bubble("喵~");
        assert_eq!(
            e,
            PetEvent::ShowBubble {
                text: "喵~".into()
            }
        );
    }

    #[test]
    fn test_event_walk_to() {
        let e = PetEvent::walk_to(150.0);
        assert_eq!(e, PetEvent::WalkTo { x: 150.0 });
    }

    #[test]
    fn test_event_serialization() {
        let e = PetEvent::react(PetMood::Happy);
        let json = serde_json::to_string(&e).unwrap();
        let parsed: PetEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, e);
    }

    #[test]
    fn test_process_button_start() {
        let events = process_button(11);
        assert!(!events.is_empty());
        assert!(matches!(events[0], PetEvent::Notify { .. }));
    }

    #[test]
    fn test_process_button_unknown() {
        let events = process_button(99);
        assert!(events.is_empty());
    }

    #[test]
    fn test_process_button_a_is_praise() {
        let events = process_button(0);
        assert!(!events.is_empty());
        assert_eq!(events[0], PetEvent::react(PetMood::Happy));
    }

    #[test]
    fn test_pending_chat_default_empty() {
        let pc = SharedPendingChat::new();
        assert!(pc.pending.lock().unwrap().is_none());
    }

    #[test]
    fn test_pending_chat_submit_and_take() {
        let pc = SharedPendingChat::new();
        *pc.pending.lock().unwrap() = Some("你好 AI".into());
        let taken = pc.pending.lock().unwrap().take();
        assert_eq!(taken, Some("你好 AI".to_string()));
        assert!(pc.pending.lock().unwrap().is_none());
    }

    #[test]
    fn test_pending_chat_submit_overwrites() {
        let pc = SharedPendingChat::new();
        *pc.pending.lock().unwrap() = Some("第一条".into());
        *pc.pending.lock().unwrap() = Some("第二条".into());
        assert_eq!(
            pc.pending.lock().unwrap().take(),
            Some("第二条".to_string())
        );
    }

    #[test]
    fn test_name_to_bit_mapping() {
        assert_eq!(name_to_bit("A"), Some(0));
        assert_eq!(name_to_bit("Home"), Some(12));
        assert_eq!(name_to_bit("Invalid"), None);
    }

    #[test]
    fn test_held_modifier_press_release() {
        let mut hm = HeldModifier::new(0x12);
        assert!(!hm.held);
        hm.press();
        assert!(hm.held);
        hm.release();
        assert!(!hm.held);
    }

    #[test]
    fn test_held_combo_detect() {
        let mut hc = HeldCombo::new();
        let (pressed, released) = hc.detect(true);
        assert!(pressed && !released);
        assert!(hc.held);
        let (p2, r2) = hc.detect(false);
        assert!(!p2 && r2);
        assert!(!hc.held);
    }
}
