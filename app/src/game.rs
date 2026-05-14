//! 迷你游戏窗口与生命周期管理。
//!
//! 本模块负责创建全屏透明 game 窗口、保存当前 `GameDef`、接收前端结束回调，
//! 并把游戏结果同步回宠物状态。实际游戏逻辑运行在前端 `game_engine.js`，
//! core crate 只提供可序列化配置和参数边界。

use ai_pad_core::bridge::PetStateName;
use ai_pad_core::minigame::{validate_game_def, GameDef};
use ai_pad_core::pet_event::{PetEvent, PetMode, PetMood};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

/// 游戏窗口共享状态。
pub struct SharedGame {
    pub starting: AtomicBool,
    pub active: AtomicBool,
    pub startup_seq: std::sync::atomic::AtomicU64,
    pub current_def: Mutex<Option<GameDef>>,
    pub hidden_windows: Mutex<Vec<String>>,
}

impl Default for SharedGame {
    fn default() -> Self {
        Self {
            starting: AtomicBool::new(false),
            active: AtomicBool::new(false),
            startup_seq: std::sync::atomic::AtomicU64::new(0),
            current_def: Mutex::new(None),
            hidden_windows: Mutex::new(Vec::new()),
        }
    }
}

/// 当前是否有游戏正在占用输入。
pub fn is_game_active(app: &AppHandle) -> bool {
    let state: tauri::State<'_, SharedGame> = app.state();
    state.active.load(Ordering::SeqCst)
}

/// 当前是否处于游戏启动或运行阶段。
pub fn is_game_busy(app: &AppHandle) -> bool {
    let state: tauri::State<'_, SharedGame> = app.state();
    state.starting.load(Ordering::SeqCst) || state.active.load(Ordering::SeqCst)
}

/// 当前游戏生命周期阶段，供日志和截图观察门控使用。
pub fn game_phase(app: &AppHandle) -> &'static str {
    let state: tauri::State<'_, SharedGame> = app.state();
    if state.starting.load(Ordering::SeqCst) {
        "starting"
    } else if state.active.load(Ordering::SeqCst) {
        "active"
    } else {
        "idle"
    }
}

/// 启动内置 Snake 游戏，供 ActionBus 和 IPC 共用。
pub fn start_default_game(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_snake())
}

/// 应用启动时预创建游戏窗口，避免从面板 IPC 回调里临时创建 WebView。
pub fn precreate_game_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("game").is_some() {
        return Ok(());
    }
    info!("[game] precreate window begin");
    create_game_window(app, 0)?;
    info!("[game] precreate window done");
    Ok(())
}

/// 启动指定游戏定义。
pub fn start_game(app: &AppHandle, def: GameDef) -> Result<(), String> {
    validate_game_def(&def)?;

    let state: tauri::State<'_, SharedGame> = app.state();
    let startup_id = state.startup_seq.fetch_add(1, Ordering::SeqCst) + 1;
    state.starting.store(true, Ordering::SeqCst);
    state.active.store(false, Ordering::SeqCst);
    {
        let mut current = state.current_def.lock().map_err(|e| e.to_string())?;
        *current = Some(def.clone());
    }

    {
        let app = app.clone();
        let title = def.title.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            let state: tauri::State<'_, SharedGame> = app.state();
            let same_startup = state.startup_seq.load(Ordering::SeqCst) == startup_id;
            let still_starting = state.starting.load(Ordering::SeqCst);
            let not_active = !state.active.load(Ordering::SeqCst);
            if same_startup && still_starting && not_active {
                state.starting.store(false, Ordering::SeqCst);
                if let Ok(mut current) = state.current_def.lock() {
                    *current = None;
                }
                let _ = set_pet_state(&app, PetStateName::Idle);
                warn!(startup_id, title = %title, "[game] 启动超时，已解除 starting 状态");
            }
        });
    }

    info!(startup_id, title = %def.title, "[game] start requested");

    info!(startup_id, "[game] set pet state begin");
    if let Err(e) = set_pet_state(app, PetStateName::GamePlay) {
        warn!(error = %e, "[game] 设置宠物 GamePlay 失败");
    }
    info!(startup_id, "[game] set pet state done");

    let start_result = (|| {
        info!(startup_id, "[game] get/create window begin");
        let window = match app.get_webview_window("game") {
            Some(w) => {
                info!(startup_id, label = "game", "[game] reuse existing window");
                w
            }
            None => create_game_window(app, startup_id)?,
        };
        info!(startup_id, "[game] get/create window done");

        info!(startup_id, "[game] position window begin");
        position_fullscreen_on_monitor(app, &window);
        info!(startup_id, "[game] position window done");
        info!(startup_id, "[game] enter focus mode begin");
        let hidden_windows = hide_companion_windows(app);
        if let Ok(mut stored) = state.hidden_windows.lock() {
            *stored = hidden_windows;
        }
        info!(startup_id, "[game] enter focus mode done");
        info!(startup_id, "[game] reload window begin");
        window
            .eval("window.location.reload()")
            .map_err(|e| e.to_string())?;
        info!(startup_id, "[game] reload window done");
        info!(startup_id, "[game] show window begin");
        window.show().map_err(|e| e.to_string())?;
        info!(startup_id, "[game] show window done");
        info!(startup_id, "[game] focus window begin");
        window.set_focus().map_err(|e| e.to_string())?;
        info!(startup_id, "[game] focus window done");
        Ok::<(), String>(())
    })();

    if let Err(e) = start_result {
        state.starting.store(false, Ordering::SeqCst);
        state.active.store(false, Ordering::SeqCst);
        if let Ok(mut current) = state.current_def.lock() {
            *current = None;
        }
        restore_companion_windows(app);
        let _ = set_pet_state(app, PetStateName::Idle);
        warn!(error = %e, "[game] 启动失败，已回滚游戏状态");
        return Err(e);
    }

    state.starting.store(false, Ordering::SeqCst);
    state.active.store(true, Ordering::SeqCst);
    info!(startup_id, title = %def.title, "[game] start completed");
    Ok(())
}

fn create_game_window(app: &AppHandle, startup_id: u64) -> Result<tauri::WebviewWindow, String> {
    info!(startup_id, url = "game.html", "[game] build window begin");
    let window = WebviewWindowBuilder::new(app, "game", WebviewUrl::App("game.html".into()))
        .title("8Bit Game")
        .inner_size(1280.0, 720.0)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    info!(startup_id, "[game] build window done");
    Ok(window)
}

fn position_fullscreen_on_monitor(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app
        .get_webview_window("pet")
        .and_then(|pet| pet.current_monitor().ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        warn!("[game] 无法获取 monitor，使用默认窗口尺寸");
        return;
    };

    let pos = monitor.position();
    let size = monitor.size();
    let _ = window.set_position(PhysicalPosition::new(pos.x, pos.y));
    let _ = window.set_size(PhysicalSize::new(size.width, size.height));
}

fn hide_companion_windows(app: &AppHandle) -> Vec<String> {
    let mut hidden = Vec::new();
    for label in ["pet", "pet-mini", "pet-snap", "bubble", "panel", "settings"] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        if !window.is_visible().unwrap_or(false) {
            continue;
        }
        if let Err(e) = window.hide() {
            warn!(error = %e, label, "[game] 隐藏辅助窗口失败");
            continue;
        }
        hidden.push(label.to_string());
    }
    info!(hidden = ?hidden, "[game] focus mode hidden windows");
    hidden
}

fn restore_companion_windows(app: &AppHandle) {
    let state: tauri::State<'_, SharedGame> = app.state();
    let labels = state
        .hidden_windows
        .lock()
        .map(|mut labels| std::mem::take(&mut *labels))
        .unwrap_or_default();

    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            if let Err(e) = window.show() {
                warn!(error = %e, label = %label, "[game] 恢复辅助窗口失败");
            }
        }
    }
}

fn set_pet_state(app: &AppHandle, state: PetStateName) -> Result<(), String> {
    let shared: tauri::State<'_, crate::commands::SharedPet> = app.state();
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    let status = crate::commands::set_state(&mut pet, state);
    let event = match status.state.as_str() {
        "gameplay" => PetEvent::set_mode(PetMode::GamePlay),
        "gamewin" => PetEvent::react(PetMood::Happy),
        "gamelose" => PetEvent::react(PetMood::Confused),
        "idle" => PetEvent::set_mode(PetMode::Idle),
        _ => PetEvent::react(PetMood::Focused),
    };
    let bus: tauri::State<'_, crate::pet_event_bus::SharedPetEventBus> = app.state();
    bus.emit(app, event);
    Ok(())
}

/// 前端请求启动默认游戏。
#[tauri::command]
pub fn cmd_start_game(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_game invoked");
    start_default_game(&app)
}

/// 前端或后续 AI 工具请求按配置启动游戏。
#[tauri::command]
pub fn cmd_start_game_with_def(app: AppHandle, def: GameDef) -> Result<(), String> {
    info!(title = %def.title, "[game] cmd_start_game_with_def invoked");
    start_game(&app, def)
}

/// 游戏窗口初始化时读取当前配置。
#[tauri::command]
pub fn cmd_get_current_game(shared: tauri::State<'_, SharedGame>) -> Result<GameDef, String> {
    let current = shared.current_def.lock().map_err(|e| e.to_string())?;
    current
        .clone()
        .ok_or_else(|| "当前没有活动游戏".to_string())
}

/// 游戏结束回调。result 支持 win / lose / cancel。
#[tauri::command]
pub fn cmd_game_end(app: AppHandle, result: String, score: u32) -> Result<(), String> {
    let normalized = result.trim().to_ascii_lowercase();
    if !matches!(normalized.as_str(), "win" | "lose" | "cancel") {
        return Err(format!("未知游戏结果: {result}"));
    }

    let state: tauri::State<'_, SharedGame> = app.state();
    state.starting.store(false, Ordering::SeqCst);
    state.active.store(false, Ordering::SeqCst);
    if let Ok(mut current) = state.current_def.lock() {
        *current = None;
    }

    if let Some(w) = app.get_webview_window("game") {
        let _ = w.hide();
    }

    let pet_state = match normalized.as_str() {
        "win" => Some(PetStateName::GameWin),
        "lose" => Some(PetStateName::GameLose),
        _ => Some(PetStateName::Idle),
    };
    if let Some(pet_state) = pet_state {
        if let Err(e) = set_pet_state(&app, pet_state) {
            warn!(error = %e, "[game] 设置结束宠物状态失败");
        }
    }
    restore_companion_windows(&app);

    info!(result = %normalized, score, "[game] 已结束");
    Ok(())
}

/// 前端调试日志桥接。
#[tauri::command]
pub fn cmd_game_log(msg: String) -> Result<(), String> {
    let preview = ai_pad_core::logging::log_preview(&msg, 120);
    info!(
        msg_chars = msg.chars().count(),
        msg_preview = %preview,
        "[game-js]"
    );
    Ok(())
}
