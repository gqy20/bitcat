//! 手柄轮询、AI 对话循环与共享业务状态管理。
//!
//! 本模块是应用运行期的中枢：80ms 手柄轮询主循环（[`gamepad_loop`]）读取 SDL2 输入，
//! 独立的 [`chat_loop`] 按接受顺序消费文字、语音和手柄请求，
//! 正文结束后由单一后台收尾线程更新情绪、长期记忆和定时画像。
//!
//! 设计上将手柄物理层（按钮检测、按住态）与 AI 对话链（上下文构建 → agent 调用 → 流式输出）
//! 解耦，确保无手柄或手柄断开时对话链仍可正常运行。
//! 对外通过 Tauri IPC 命令（`cmd_submit_chat` / `cmd_open_chat` 等）接收前端事件，
//! 对内通过 `pet-event` 通知前端宠物状态变化。
//! AI 生成保护由守卫管理，前端输入与阅读独立保持交互保护，停止回复不会退出会话。
//! 初始化失败与早退也发送完整回复结束协议，技术详情只记录在诊断日志中。
//! 请求提交时预留取消编号；停止清理排队请求并丢弃运行中的回复 future，避免继续启动工具。
//! 已经执行的外部动作无法撤销，正在执行的阻塞操作也可能继续完成。

use crate::bubble;
use crate::commands::SharedWindowState;
use crate::game_input::{emit_game_input, GameInput};
use crate::joystick::{self, SdlGamepad};
use crate::panel;
use crate::pet_event_bus::SharedPetEventBus;
use crate::tts;
use crate::voice;
use bitcat_core::action::{ActionConfig, ActionDef};
use bitcat_core::agent::{
    parse_tool_failure_stop, AgentStreamEvent, ChatError, PetAgent, ToolPhase,
};
use bitcat_core::bridge::{handle_button_press, PetCommand};
use bitcat_core::device::button_name;
use bitcat_core::hotkey;
use bitcat_core::logging::log_preview;
use bitcat_core::memory::{LongTermMemory, MemoryStore, ProfileStore};
use bitcat_core::pet_event::{
    agent_status_to_pet_event, tool_event_to_pet_event, PetEvent, PetMode, PetMood,
    PetNotificationKind,
};
use bitcat_core::user_profile::UserProfile;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// 根据按钮索引生成本地互动事件，AI 状态由实际消费请求的正文循环发送。
pub fn process_button(button_index: u32) -> Vec<PetEvent> {
    let (_agent_msg, pet_cmd) = handle_button_press(button_index, "");
    let mut events = Vec::new();
    match button_index {
        10 => events.push(PetEvent::set_mode(PetMode::Sleep)),
        0 => {
            events.push(PetEvent::react(PetMood::Happy));
            bitcat_core::points::award(bitcat_core::points::PointsEventKind::PetPraised, None);
        }
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

/// 初始化失败时回应用户请求，避免输入框已经开始等待却没有结束消息。
fn show_agent_unavailable(app: &AppHandle, request: &bubble::ChatRequest) {
    if let Err(e) = bubble::show_chat_message(
        app,
        request,
        "对话暂时不可用。AI 连接尚未准备好，请到设置检查 API Key 和服务地址，保存后重启应用。",
    ) {
        warn!(error = %e, "AI unavailable feedback failed");
    }
}

/// 已开始的回复写入静态说明后结束，正文写入失败时也结束前端等待。
fn finish_chat_feedback(app: &AppHandle, request_id: u64, message: &str) {
    if let Err(e) = bubble::append_bubble_chunk(app, request_id, message) {
        warn!(error = %e, "chat feedback append failed");
    }
    if let Err(e) = bubble::finalize_bubble(app, request_id) {
        warn!(error = %e, "chat feedback finalization failed");
    }
}

// ========================================================================
// 聊天输入系统（前端提交 → chat_loop 消费）
// ========================================================================

/// 已提交的对话请求，编号从排队到运行保持不变，停止时不会因取出队列而失效。
pub type PendingChatRequest = bubble::ChatRequest;

/// 所有已接受请求的 FIFO 队列，只有 chat_loop 可以消费。
///
/// 编号、原文和来源在排队期间保持不变，停止按编号清理，不覆盖其他输入。
pub struct SharedPendingChat {
    pending: Mutex<VecDeque<PendingChatRequest>>,
}

impl SharedPendingChat {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
        }
    }

    /// 在接受请求的生命周期锁内调用，保留每条请求及其接受顺序。
    fn set(&self, request: PendingChatRequest) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|e| e.to_string())?;
        pending.push_back(request);
        Ok(())
    }

    /// 短锁取走请求；运行时仍检查原编号是否已经取消。
    fn take(&self) -> Option<PendingChatRequest> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.pop_front())
    }

    /// 清理停止操作覆盖的待执行请求，保留停止后新提交的请求。
    fn cancel_through(&self, generation: u64) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|e| e.to_string())?;
        pending.retain(|request| request.request_id > generation);
        Ok(())
    }
}

impl Default for SharedPendingChat {
    fn default() -> Self {
        Self::new()
    }
}

/// 已提交和正在运行的 AI 对话取消状态。
///
/// 提交请求时预留 generation，执行时使用原编号；停止同时覆盖排队和运行的请求。
/// 运行中的模型流通过 select 丢弃 future，已开始的外部动作无法撤销。
pub struct SharedChatCancel {
    current_generation: AtomicU64,
    cancelled_until_generation: AtomicU64,
    cancellation_changed: tokio::sync::Notify,
    lifecycle: Mutex<()>,
}

impl SharedChatCancel {
    pub fn new() -> Self {
        Self {
            current_generation: AtomicU64::new(0),
            cancelled_until_generation: AtomicU64::new(0),
            cancellation_changed: tokio::sync::Notify::new(),
            lifecycle: Mutex::new(()),
        }
    }

    /// 为新请求预留编号，必须在写入待执行队列之前调用。
    pub fn begin_chat(&self) -> u64 {
        let _guard = self.lifecycle.lock().expect("chat lifecycle lock poisoned");
        self.current_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn cancel_current(&self) -> u64 {
        self.cancel_through(self.latest_request_id())
    }

    /// 当前已预留的编号，不能据此跳过 FIFO 中较早但仍有效的请求。
    pub fn latest_request_id(&self) -> u64 {
        self.current_generation.load(Ordering::SeqCst)
    }

    /// 取消截至给定编号的请求，迟到的停止不会取消之后提交的请求。
    pub fn cancel_through(&self, requested_id: u64) -> u64 {
        let _guard = self.lifecycle.lock().expect("chat lifecycle lock poisoned");
        let generation = requested_id.min(self.latest_request_id());
        if generation > 0 {
            self.cancelled_until_generation
                .fetch_max(generation, Ordering::SeqCst);
            self.cancellation_changed.notify_waiters();
        }
        generation
    }

    /// 与接受新请求和停止共用一个锁，确保旧情绪检查与发送之间不会插入新会话。
    pub(crate) fn publish_if_current<T>(
        &self,
        request_id: u64,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        let _guard = self.lifecycle.lock().ok()?;
        if self.latest_request_id() != request_id
            || self.is_cancelled(request_id)
            || crate::shutdown::is_requested()
        {
            return None;
        }
        Some(publish())
    }

    pub fn is_cancelled(&self, generation: u64) -> bool {
        generation > 0 && generation <= self.cancelled_until_generation.load(Ordering::SeqCst)
    }

    /// 停止后丢弃整个流式 future，避免仅隐藏输出却继续启动后续工具。
    pub(crate) async fn run_until_cancelled<T>(
        &self,
        generation: u64,
        work: impl std::future::Future<Output = T>,
    ) -> Option<T> {
        tokio::select! {
            biased;
            _ = async {
                loop {
                    let notified = self.cancellation_changed.notified();
                    tokio::pin!(notified);
                    // 先登记等待者再检查编号，防止检查与 await 之间丢失停止通知。
                    notified.as_mut().enable();
                    if self.is_cancelled(generation) {
                        return;
                    }
                    notified.await;
                }
            } => None,
            result = work => Some(result),
        }
    }
}

impl Default for SharedChatCancel {
    fn default() -> Self {
        Self::new()
    }
}

/// 前端触发的"提交聊天消息"命令，通过 ActionBus 写入 [`SharedPendingChat`]。
#[tauri::command]
pub async fn cmd_submit_chat(
    app: AppHandle,
    text: String,
) -> Result<bubble::ChatSubmission, String> {
    crate::action_bus::ActionBus::submit_chat(
        &app,
        text,
        bubble::ChatSource::Text,
        crate::action_bus::ActionSource::Frontend {
            cmd: "cmd_submit_chat".into(),
        },
    )
}

/// 停止排队或运行的请求并通知 bubble 进入停止态，已执行的外部动作无法撤销。
#[tauri::command]
pub async fn cmd_cancel_chat(
    app: AppHandle,
    through_request_id: Option<u64>,
) -> Result<bubble::ChatCancelled, String> {
    let cancel: State<'_, SharedChatCancel> = app.state();
    let generation =
        cancel.cancel_through(through_request_id.unwrap_or_else(|| cancel.latest_request_id()));
    info!(generation, "[chat] cancel requested");
    let pending: State<'_, SharedPendingChat> = app.state();
    if let Err(e) = pending.cancel_through(generation) {
        warn!(error = %e, "cancel pending chat cleanup failed");
    }
    // 生成保护由运行中的 future 实际结束后释放，避免停止与最后一个回调之间被通知覆盖。
    let payload = bubble::ChatCancelled {
        request_id: generation,
    };
    let _ = app.emit_to("bubble", "bubble-cancelled", payload);
    Ok(payload)
}

/// 原子接受请求并发布排队事件，正文开始后才由 chat_loop 切换当前问题。
pub(crate) fn queue_chat(
    app: &AppHandle,
    text: String,
    source: bubble::ChatSource,
) -> Result<bubble::ChatSubmission, String> {
    let pending: State<SharedPendingChat> = app.state();
    let cancel: State<SharedChatCancel> = app.state();
    accept_chat(&pending, &cancel, text, source, |request| {
        if let Err(error) = app.emit_to("bubble", "bubble-queued", request.clone()) {
            warn!(%error, request_id = request.request_id, "chat queued event failed");
        }
    })
}

pub(crate) fn accept_chat(
    pending: &SharedPendingChat,
    cancel: &SharedChatCancel,
    text: String,
    source: bubble::ChatSource,
    accepted: impl FnOnce(&PendingChatRequest),
) -> Result<bubble::ChatSubmission, String> {
    let user_text = text.trim().to_string();
    if user_text.is_empty() {
        return Err("消息不能为空".into());
    }
    let _guard = cancel.lifecycle.lock().map_err(|e| e.to_string())?;
    if crate::shutdown::is_requested() {
        return Err("应用正在关闭".into());
    }
    let request = PendingChatRequest {
        request_id: cancel.current_generation.fetch_add(1, Ordering::SeqCst) + 1,
        user_text,
        source,
    };
    pending.set(request.clone())?;
    accepted(&request);
    Ok(bubble::ChatSubmission {
        request_id: request.request_id,
    })
}

/// 原子性地取出并清空待消费的聊天消息，返回 `None` 表示无新消息。
pub fn take_pending_chat(state: &State<'_, SharedPendingChat>) -> Option<PendingChatRequest> {
    state.take()
}

/// 唯一正文循环的消费步骤；执行完成后立即返回，后台收尾不参与此执行链。
pub(crate) fn consume_next(
    pending: &SharedPendingChat,
    cancel: &SharedChatCancel,
    execute: impl FnOnce(&PendingChatRequest),
) -> bool {
    let Some(request) = pending.take() else {
        return false;
    };
    if cancel.is_cancelled(request.request_id) {
        return false;
    }
    execute(&request);
    true
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
/// | `memory` | chat_loop（下一轮开始前同步） | chat_loop |
/// | `long_term` | 收尾 worker（持久化事务后刷新缓存） | chat_loop 每轮从最新文件刷新 |
/// | `profile` | 收尾 worker（聚合后更新） | chat_loop |
/// | `user_profile` | 初始化与设置页 | chat_loop |
/// | `last_aggregation` | 收尾 worker | 收尾 worker |
pub struct SharedChatCore {
    /// 短期对话记忆（滚动窗口），仅 chat_loop 执行对话时读写。
    pub memory: Mutex<MemoryStore>,
    /// 最新长期记忆缓存；写入必须走共享持久化事务，缓存不能直接覆盖文件。
    pub long_term: Mutex<LongTermMemory>,
    /// 自动聚合的用户画像，优先级低于 `user_profile`。
    pub profile: Mutex<ProfileStore>,
    /// 用户显式声明的身份信息（config/user.yml），为空时回退到 `profile`。
    pub user_profile: Mutex<UserProfile>,
    /// 上次画像聚合时间戳，由后台收尾 worker 检查和更新。
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
                    info!("AI Agent 初始化成功 (BitCat)");
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
    if !bitcat_core::logging::frontend_log_allowed("pet", std::time::Duration::from_millis(120)) {
        return Ok(());
    }
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

/// 轻量"进入 chat"命令：保护用户输入或阅读，不负责开窗口/展示 UI。
///
/// 与 cmd_open_chat 的区别：
/// - cmd_open_chat 走"点击嘴巴"路径，会创建窗口 + eval showInput
/// - cmd_enter_chat 给前端用：用户主动聊天或阅读时通知后端避让截图 / Vision；
///   生成结束与停止不会释放这份保护，直到 cmd_exit_chat 或收起窗口。
#[tauri::command]
pub async fn cmd_enter_chat(app: AppHandle) -> Result<(), String> {
    let state: State<bubble::SharedBubble> = app.state();
    let was_active = state.is_chat_active();
    state.set_interaction_active(true);
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

    let mut action_config = ActionConfig::load("config/actions.yml").ok();
    let ac = action_config.as_ref().map(|c| c.actions.len()).unwrap_or(0);
    info!(action_count = ac, "已加载 {ac} 个动作绑定");

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
                if last_warn.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(60)) {
                    warn!(
                        enumerated = pads.len(),
                        "未检测到真正的游戏手柄（已自动跳过键鼠接收器等），后台继续重试..."
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
        info!("BitCat 启动（手柄已就绪）");

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
            let watch_visible = !panel_visible
                && app
                    .get_webview_window("agent-watch")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
            let game_active = crate::game::is_game_active(app);

            let buttons = gamepad.read_buttons();
            let new_presses = (buttons ^ prev_buttons) & buttons;
            let releases = (buttons ^ prev_buttons) & prev_buttons;

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
                            let is_combat_game = matches!(
                                crate::game::current_game_type(app),
                                Some(
                                    bitcat_core::minigame::MinigameType::Battle
                                        | bitcat_core::minigame::MinigameType::Arena
                                        | bitcat_core::minigame::MinigameType::Invasion
                                )
                            );
                            if is_combat_game {
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
                                        if matches!(
                                            crate::game::current_game_type(app),
                                            Some(bitcat_core::minigame::MinigameType::Snake)
                                        ) {
                                            emit_game_input(app, GameInput::Boost { active: true });
                                        }
                                    }
                                    "B" => {
                                        info!("→ 游戏取消");
                                        emit_game_input(app, GameInput::Cancel);
                                    }
                                    "X" => {
                                        info!("→ 游戏选项切换");
                                        emit_game_input(app, GameInput::Cycle { dir: 1 });
                                    }
                                    "Y" => {
                                        info!("→ 游戏撤销");
                                        emit_game_input(app, GameInput::Undo);
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
                                    let _ = app.emit_to("panel", "panel-confirm", ());
                                }
                                "B" => {
                                    info!("→ 面板关闭");
                                    let _ = app.emit_to("panel", "panel-close", ());
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if watch_visible {
                            match name {
                                "A" => {
                                    info!("→ Agent Watch 展开焦点卡");
                                    let _ = app.emit_to("agent-watch", "agent-watch-confirm", ());
                                }
                                "B" => {
                                    info!("→ Agent Watch 收起焦点");
                                    let _ = app.emit_to("agent-watch", "agent-watch-back", ());
                                }
                                _ => {}
                            }
                            continue;
                        }

                        let (agent_msg, pet_cmd) = handle_button_press(idx, "");

                        // 舞蹈命令：走 bridge 统一播放管线，启用 is_dancing 状态
                        if let Some(PetCommand::PlayDance { name }) = &pet_cmd {
                            info!(dance = %name, "[gamepad] Y 键 → 播放舞蹈");
                            if bitcat_core::dance::load_dance(name).is_err() {
                                warn!(dance = %name, "[gamepad] 舞蹈不存在，无法播放");
                            } else {
                                let req = bitcat_core::dance::PlayDanceRequest {
                                    name: name.clone(),
                                    loops: Some(1), // Y 键默认单次
                                    duration_ms: None,
                                };
                                if let Err(e) = bitcat_core::dance::request_play_dance(req) {
                                    warn!(error = %e, "手柄触发舞蹈失败");
                                }
                            }
                        }

                        let events = process_button(idx);
                        for evt in events {
                            emit_pet_event(app, evt);
                        }

                        if let Some(msg) = &agent_msg {
                            if let Err(error) = crate::action_bus::ActionBus::submit_chat(
                                app,
                                msg.clone(),
                                bubble::ChatSource::Gamepad,
                                crate::action_bus::ActionSource::Gamepad {
                                    button: name.to_string(),
                                },
                            ) {
                                warn!(%error, "gamepad chat request not accepted");
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

            if game_active && releases != 0 {
                for bit in 0..32 {
                    if releases & (1 << bit) != 0 {
                        let name = button_name(bit as usize).unwrap_or("?");
                        if name == "A"
                            && matches!(
                                crate::game::current_game_type(app),
                                Some(bitcat_core::minigame::MinigameType::Snake)
                            )
                        {
                            info!("→ 贪吃蛇加速结束");
                            emit_game_input(app, GameInput::Boost { active: false });
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
                            if let Err(error) = crate::action_bus::ActionBus::submit_chat(
                                app,
                                text,
                                bubble::ChatSource::Voice,
                                crate::action_bus::ActionSource::Gamepad {
                                    button: "voice".into(),
                                },
                            ) {
                                warn!(%error, "voice chat request not accepted");
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
                    } else {
                        emit_game_input(app, GameInput::Direction { dx: 0, dy: 0 });
                    }
                }
            } else if panel_visible {
                if hat != prev_hat {
                    if let Some((dx, dy)) = hat {
                        info!(dx = dx, dy = dy, "→ 面板导航");
                        let _ = app.emit_to("panel", "panel-nav", (dx, dy));
                    }
                }
            } else if watch_visible {
                if hat != prev_hat {
                    if let Some((dx, dy)) = hat {
                        info!(dx = dx, dy = dy, "→ Agent Watch 导航");
                        let _ = app.emit_to("agent-watch", "agent-watch-nav", (dx, dy));
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

/// 唯一的正文执行循环，消费文字、语音和手柄 FIFO，同步写入短期记忆。
///
/// 与 gamepad_loop **平级独立运行**。没有手柄、手柄断开、手柄未识别时，
/// 本循环按常规节奏消费所有入口的请求，提取与画像聚合由后台 worker 执行。
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

    loop {
        // --- 1. 消费 bubble 聊天输入 ---
        if crate::shutdown::is_requested() {
            info!("[chat_loop] shutdown requested, exiting");
            break;
        }
        let pending: State<SharedPendingChat> = app.state();
        let cancel: State<SharedChatCancel> = app.state();
        consume_next(&pending, &cancel, |request| {
            let agent_state: State<SharedAgent> = app.state();
            if cancel.is_cancelled(request.request_id) {
                info!(
                    generation = request.request_id,
                    "[chat] queued request cancelled before start"
                );
            } else if let Some(ag) = agent_state.get_or_init() {
                let core: State<SharedChatCore> = app.state();
                let preview = log_preview(&request.user_text, 60);
                info!(
                    msg_chars = request.user_text.chars().count(),
                    msg_preview = %preview,
                    "[chat] bubble input received"
                );
                run_ai_chat_request(&rt, ag, app, request, "[chat]", &core);
            } else {
                let preview = log_preview(&request.user_text, 60);
                warn!(
                    msg_chars = request.user_text.chars().count(),
                    msg_preview = %preview,
                    "[chat] AI Agent 未就绪，结束本轮对话并展示设置说明"
                );
                show_agent_unavailable(app, request);
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

/// RAII 守卫：监督当前请求的结束路径，按同一编号完成尚未结束的流。
///
/// **设计意图**：AI 对话期间截屏线程应跳过 Vision 分析（避免并发 token 消耗和
/// 内容冲突）。无论正文正常返回、提前退出还是 panic，守卫都会完成自己的请求；
/// 已结束或过期的守卫不会重复通知，也不会释放下一轮请求或用户交互的保护。
struct ChatActiveGuard {
    app: tauri::AppHandle,
    log_prefix: String,
    request_id: u64,
}

impl ChatActiveGuard {
    /// 记录本轮所有权；生成保护由 start_streaming_bubble 开启。
    fn new(app: &tauri::AppHandle, request_id: u64, log_prefix: &str) -> Self {
        Self {
            app: app.clone(),
            log_prefix: log_prefix.to_string(),
            request_id,
        }
    }
}

impl Drop for ChatActiveGuard {
    fn drop(&mut self) {
        if let Err(error) = bubble::finalize_bubble(&self.app, self.request_id) {
            warn!(%error, request_id = self.request_id, "chat guard finalization failed");
        }
        info!(
            request_id = self.request_id,
            prefix = %self.log_prefix,
            "chat request scope closed"
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
/// 截屏互斥：函数入口开启生成保护，RAII guard 在 panic/return 时只释放生成保护。
/// 执行已经预留编号的请求，排队消息不得重新分配编号而绕过停止操作。
fn run_ai_chat_request(
    rt: &tokio::runtime::Runtime,
    agent: &PetAgent,
    app: &tauri::AppHandle,
    request: &PendingChatRequest,
    log_prefix: &str,
    core: &SharedChatCore,
) {
    let cancel_state: State<SharedChatCancel> = app.state();
    let chat_generation = request.request_id;
    if cancel_state.is_cancelled(chat_generation) {
        info!(
            generation = chat_generation,
            "{log_prefix}chat cancelled before start"
        );
        return;
    }
    let msg = request.user_text.as_str();
    let tag = if log_prefix.is_empty() { "" } else { " " };
    let msg_preview = log_preview(msg, 60);
    info!(
        model = %agent.config.model,
        msg_chars = msg.chars().count(),
        msg_preview = %msg_preview,
        "{log_prefix}AI chat started"
    );

    // RAII 锁：整个 chat 期间阻止截屏线程进入 Vision 分析；panic 或 early return 时自动释放
    let _chat_guard = ChatActiveGuard::new(app, chat_generation, log_prefix);

    if let Err(e) = bubble::start_streaming_bubble(app, request) {
        warn!(error = %e, "{log_prefix}气泡启动错误");
        finish_chat_feedback(
            app,
            chat_generation,
            "这次没能回复。对话窗口没有准备好，请重启应用后再试。",
        );
        return;
    }

    let prompts_cfg = bitcat_core::prompts::PromptsConfig::load();
    let memory_config = &prompts_cfg.memory;
    let long_term_budget_chars = prompts_cfg.memory_v2.retrieve_budget_chars;

    // ---- 构建上下文：各字段单独短锁 ----
    let ctx = match core.memory.lock() {
        Ok(g) => g.build_context(memory_config),
        Err(e) => {
            warn!(error = %e, "memory 锁中毒，跳过上下文");
            finish_chat_feedback(
                app,
                chat_generation,
                "这次没能回复。对话记录暂时读不了，请重启应用后再试。",
            );
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
    let latest_long_term = match LongTermMemory::load_checked() {
        Ok(store) => store,
        Err(error) => {
            warn!(%error, "读取最新长期记忆失败");
            finish_chat_feedback(
                app,
                chat_generation,
                "这次没能回复。记忆暂时读不了，请到设置查看记录后再试。",
            );
            return;
        }
    };
    let long_term_ctx = latest_long_term.retrieve_with(
        &bitcat_core::memory::LongTermMemoryQuery {
            text: msg.to_string(),
            ..Default::default()
        },
        long_term_budget_chars,
    );
    if let Ok(mut cache) = core.long_term.lock() {
        *cache = latest_long_term;
    }
    let summary_store = bitcat_core::screen_summary::ScreenSummaryStore::load();
    let summary_config = bitcat_core::prompts::PromptsConfig::load().screen_summary;
    let summary_ctx = summary_store.build_context(&summary_config);
    let recent_ctx = bitcat_core::screenshot::build_recent_analyses_context(10, 1500);
    let camera_ctx = bitcat_core::camera_observation::build_recent_camera_context(6, 1200);
    let observation_policy = if !recent_ctx.is_empty() && !camera_ctx.is_empty() {
        [
            "[综合观察说明]",
            "最近截图观察描述屏幕内容；最近摄像头观察描述用户是否在位、是否看向屏幕等弱信号。",
            "二者时间相近但可能有少量延迟；回答时请把它们作为同一观察周期的互补证据，避免过度推断情绪、健康或身份。",
            "[/综合观察说明]\n",
        ]
        .join("\n")
    } else {
        String::new()
    };
    let context_policy = [
        "[上下文优先级]",
        "如果上下文互相冲突，按以下顺序判断：",
        "1. 用户当前这句话和工具实时结果最优先。",
        "2. 本轮/最近对话记录优先于长期记忆候选。",
        "3. 显式用户画像优先于自动聚合画像。",
        "4. 长期记忆候选可能过期；涉及提醒、任务状态、文件状态时应优先调用工具核对。",
        "不要把旧记忆里的失败、能力限制或历史承诺当作当前事实。",
        "[/上下文优先级]\n",
    ]
    .join("\n");
    let context_parts: Vec<&str> = [
        &context_policy,
        &user_profile_ctx,
        &profile_ctx,
        &long_term_ctx,
        &ctx,
        &recent_ctx,
        &camera_ctx,
        &observation_policy,
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
        camera_ctx_chars = camera_ctx.chars().count(),
        summary_ctx_chars = summary_ctx.chars().count(),
        enriched_msg_chars = enriched_msg.chars().count(),
        "{log_prefix}chat context assembled"
    );

    // ---- 流式 IO：不持锁 ----
    let app_for_chunks = app.clone();
    let cancel_for_stream: tauri::State<'_, SharedChatCancel> = app.state();
    let prefix = log_prefix.to_string();
    let prefix_for_log = prefix.clone();
    let tool_summaries = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let tool_summaries_for_stream = tool_summaries.clone();
    emit_pet_event(app, PetEvent::ai_thinking());
    let stream_result = rt.block_on(cancel_state.run_until_cancelled(
        chat_generation,
        agent.chat_stream(&enriched_msg, move |event| match event {
            AgentStreamEvent::Text { text } => {
                if cancel_for_stream.is_cancelled(chat_generation) {
                    return;
                }
                trace!(
                    chunk_chars = text.chars().count(),
                    "{prefix_for_log}{tag}AI chunk"
                );
                let _ = bubble::append_bubble_chunk(&app_for_chunks, chat_generation, &text);
            }
            AgentStreamEvent::Status { status } => {
                if cancel_for_stream.is_cancelled(chat_generation) {
                    return;
                }
                debug!(status = ?status, "{prefix_for_log}{tag}AI stream status");
                emit_pet_event(&app_for_chunks, agent_status_to_pet_event(status));
            }
            AgentStreamEvent::Tool { event } => {
                if cancel_for_stream.is_cancelled(chat_generation) {
                    return;
                }
                debug!(
                    tool = %event.tool_name,
                    phase = ?event.phase,
                    "{prefix_for_log}{tag}AI tool event"
                );
                if let Some(pet_event) = tool_event_to_pet_event(&event) {
                    emit_pet_event(&app_for_chunks, pet_event);
                }
                if event.phase != ToolPhase::Planned {
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
                }
                if event.tool_name == "create_reminder"
                    && event.phase == ToolPhase::Finished
                    && event.success == Some(true)
                {
                    let _ = app_for_chunks.emit_to("settings", "reminders-updated", ());
                }
                let _ = bubble::emit_tool_event(
                    &app_for_chunks,
                    bubble::BubbleToolPayload {
                        request_id: chat_generation,
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
        }),
    ));
    let chat_cancelled = cancel_state.is_cancelled(chat_generation);
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

    let Some(stream_result) = stream_result.filter(|_| !chat_cancelled) else {
        let _ = bubble::finalize_bubble(app, chat_generation);
        info!(generation = chat_generation, "{prefix}AI chat cancelled");
        return;
    };

    match stream_result {
        Ok(reply) => {
            let completed_at = std::time::Instant::now();
            let _ = bubble::finalize_bubble(app, chat_generation);
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
            bitcat_core::points::award(bitcat_core::points::PointsEventKind::ChatCompleted, None);
            if request.source == bubble::ChatSource::Voice {
                bitcat_core::points::award(bitcat_core::points::PointsEventKind::VoiceChat, None);
            }
            let reply_for_tts = reply.clone();
            let tts_on = bitcat_core::app_settings::AppSettings::load()
                .appearance
                .tts_enabled;
            if tts_on {
                std::thread::spawn(move || {
                    tts::speak(&reply_for_tts);
                });
            }

            let summaries = tool_summaries.lock().map(|g| g.clone()).unwrap_or_default();
            let worker: State<crate::chat_reaction::SharedChatReaction> = app.state();
            worker.submit(crate::chat_reaction::ReactionJob {
                request: request.clone(),
                reply,
                config: agent.config.clone(),
                tool_summaries: summaries,
                max_entries: prompts_cfg.memory_v2.long_term_max_entries,
                completed_at,
            });
        }
        Err(e) => {
            // 结构化诊断日志（完整信息写入日志，不暴露给用户）
            warn!(
                model = %agent.config.model,
                error_kind = %e.short_kind(),
                error_reason = %match &e {
                    ChatError::RecoverableStream { reason, .. } | ChatError::Fatal { reason, .. } => reason.as_str(),
                },
                error_original = %log_preview(e.original_message(), 300),
                accumulated = e.accumulated_chars(),
                "{log_prefix} AI 对话流错误"
            );

            // 工具连续失败走独立分支（结构化错误，优先级高于 ChatError 分类）
            // 注意：tool_failure_stop 格式是 "tool_failure_stop:name:detail"，不是 ChatError
            let user_reply = if let Some((tool_name, _detail)) =
                parse_tool_failure_stop(&e.to_string())
            {
                if tool_name == "create_reminder" {
                    "提醒没有创建成功。这次操作没能完成，请检查提醒内容后再试。".to_string()
                } else {
                    "这次操作没有完成。执行过程遇到问题，请检查授权和设置后再试。".to_string()
                }
            } else {
                // 根据 ChatError 分类生成用户友好消息
                match &e {
                    ChatError::RecoverableStream { .. } => {
                        // 部分恢复：模型说了些话但没说完
                        "回复中途断开了。连接可能不稳定，请稍后重新发送这条消息。".to_string()
                    }
                    ChatError::Fatal { reason, .. } => match reason.as_str() {
                        "network" => "暂时连不上 AI 服务。请检查网络和服务地址后再试。".to_string(),
                        "auth" => {
                            "AI 服务没有接受这次请求。API Key 可能不可用，请到设置检查后再试。"
                                .to_string()
                        }
                        "rate_limit" => {
                            "AI 服务暂时无法继续回复。请求太频繁，请稍等片刻再试。".to_string()
                        }
                        "max_turns" => {
                            "这次没有完成回复。需要处理的步骤太多，请把问题拆小后再试。".to_string()
                        }
                        _ => "这次没能完成回复。AI 服务返回了异常，请稍后再试。".to_string(),
                    },
                }
            };
            finish_chat_feedback(app, chat_generation, &format!("\n\n{user_reply}"));
            if let Ok(mut memory) = core.memory.lock() {
                memory.record_conversation(msg, &user_reply, memory_config);
                if let Err(save_err) = memory.save() {
                    warn!(error = %save_err, "保存失败对话记忆失败");
                }
            } else {
                warn!("memory 锁中毒，跳过失败对话记忆写入");
            }
            emit_pet_event(
                app,
                PetEvent::Notify {
                    kind: PetNotificationKind::ToolFailed,
                    body: Some(user_reply.clone()),
                    ttl_ms: Some(15_000),
                    refresh: true,
                },
            );
        }
    }
}

/// 执行原始动作定义（未迁移到 ActionBus 的遗留路径）。
///
/// 处理 launch / script / hotkey / voice 四种类型。`ModifierTab` 按住态的按键
/// 状态由 `alt_tab` / `ctrl_tab` 参数维护，不经过 ActionBus。
fn execute_action(
    action: &ActionDef,
    defaults: &bitcat_core::action::Defaults,
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
            let _ = bitcat_core::action::launch_program(
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

    pub fn press_keys(&mut self, config: &bitcat_core::action::ActionConfig) {
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
        assert!(events.is_empty(), "排队中的按键请求不提前宣称开始思考");
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
        assert!(pc.take().is_none());
    }

    #[test]
    fn test_all_sources_preserve_fifo_and_metadata() {
        let pending = SharedPendingChat::new();
        let cancel = SharedChatCancel::new();
        let mut accepted = Vec::new();
        for (text, source) in [
            ("文字原话", bubble::ChatSource::Text),
            ("语音原话", bubble::ChatSource::Voice),
            ("手柄原话", bubble::ChatSource::Gamepad),
        ] {
            let ack = accept_chat(&pending, &cancel, text.into(), source, |request| {
                accepted.push(request.clone())
            })
            .unwrap();
            assert_eq!(accepted.last().unwrap().request_id, ack.request_id);
        }
        for request in accepted {
            assert_eq!(pending.take(), Some(request));
        }
        assert!(pending.take().is_none());
    }

    #[test]
    fn test_concurrent_producers_preserve_every_accepted_request_in_order() {
        use std::sync::{Arc, Barrier};
        let pending = Arc::new(SharedPendingChat::new());
        let cancel = Arc::new(SharedChatCancel::new());
        let barrier = Arc::new(Barrier::new(4));
        let mut handles = Vec::new();
        for (producer, source) in [
            bubble::ChatSource::Text,
            bubble::ChatSource::Voice,
            bubble::ChatSource::Gamepad,
            bubble::ChatSource::Text,
        ]
        .into_iter()
        .enumerate()
        {
            let pending = pending.clone();
            let cancel = cancel.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for item in 0..16 {
                    accept_chat(
                        &pending,
                        &cancel,
                        format!("{producer}:{item}"),
                        source,
                        |_| {},
                    )
                    .unwrap();
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let requests = std::iter::from_fn(|| pending.take()).collect::<Vec<_>>();
        assert_eq!(requests.len(), 64);
        for (index, request) in requests.iter().enumerate() {
            assert_eq!(request.request_id, index as u64 + 1);
        }
        let texts = requests
            .iter()
            .map(|r| r.user_text.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(texts.len(), 64);
    }

    #[test]
    fn test_stop_clears_queued_requests_and_preserves_later_submission() {
        let pending = SharedPendingChat::new();
        let cancel = SharedChatCancel::new();
        let first = accept_chat(
            &pending,
            &cancel,
            "停止前第一句".into(),
            bubble::ChatSource::Text,
            |_| {},
        )
        .unwrap()
        .request_id;
        let second = accept_chat(
            &pending,
            &cancel,
            "停止前第二句".into(),
            bubble::ChatSource::Voice,
            |_| {},
        )
        .unwrap()
        .request_id;
        let third = accept_chat(
            &pending,
            &cancel,
            "停止后新发的消息".into(),
            bubble::ChatSource::Text,
            |_| {},
        )
        .unwrap()
        .request_id;
        let stopped = cancel.cancel_through(second);
        pending.cancel_through(stopped).unwrap();
        assert!(cancel.is_cancelled(first));
        assert!(cancel.is_cancelled(second));
        assert!(!cancel.is_cancelled(third));
        assert_eq!(pending.take().unwrap().request_id, third);
        assert!(pending.take().is_none());
        // 迟到的旧停止不能取消新编号。
        assert_eq!(cancel.cancel_through(first), first);
        assert!(!cancel.is_cancelled(third));
    }

    #[tokio::test]
    async fn test_stop_after_queue_take_prevents_starting_stream() {
        use std::sync::atomic::AtomicBool;
        let pending = SharedPendingChat::new();
        let cancel = SharedChatCancel::new();
        let ack = accept_chat(
            &pending,
            &cancel,
            "已取走但尚未运行".into(),
            bubble::ChatSource::Text,
            |_| {},
        )
        .unwrap();
        let request = pending.take().unwrap();
        pending
            .cancel_through(cancel.cancel_through(ack.request_id))
            .unwrap();
        let started = AtomicBool::new(false);
        let result = cancel
            .run_until_cancelled(request.request_id, async {
                started.store(true, Ordering::SeqCst);
            })
            .await;
        assert!(result.is_none());
        assert!(!started.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_stop_running_stream_drops_future_before_next_tool() {
        use std::sync::atomic::AtomicBool;

        struct DropMarker<'a>(&'a AtomicBool);
        impl Drop for DropMarker<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let cancel = SharedChatCancel::new();
        let generation = cancel.begin_chat();
        let stream_started = AtomicBool::new(false);
        let future_dropped = AtomicBool::new(false);
        let next_tool_started = AtomicBool::new(false);
        let work = async {
            let _drop_marker = DropMarker(&future_dropped);
            stream_started.store(true, Ordering::SeqCst);
            std::future::pending::<()>().await;
            next_tool_started.store(true, Ordering::SeqCst);
        };
        let stop = async {
            while !stream_started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            cancel.cancel_current();
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(cancel.run_until_cancelled(generation, work), stop)
        })
        .await
        .expect("停止应及时结束流式等待");

        assert!(result.is_none());
        assert!(future_dropped.load(Ordering::SeqCst));
        assert!(!next_tool_started.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_stream_completion_without_stop_preserves_result() {
        let cancel = SharedChatCancel::new();
        let generation = cancel.begin_chat();
        let result = cancel
            .run_until_cancelled(generation, async { "完成回复" })
            .await;
        assert_eq!(result, Some("完成回复"));
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
