//! 迷你游戏窗口与生命周期管理。
//!
//! 本模块负责创建全屏透明 game 窗口、保存当前 `GameDef`、接收前端结束回调，
//! 并把游戏结果同步回宠物状态。实际游戏逻辑运行在前端 `game_engine.js`，
//! core crate 只提供可序列化配置和参数边界。

use ai_pad_core::bridge::PetStateName;
use ai_pad_core::minigame::{validate_game_def, GameDef};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};
use tracing::{info, warn};

/// 游戏窗口共享状态。
pub struct SharedGame {
    pub active: AtomicBool,
    pub current_def: Mutex<Option<GameDef>>,
}

impl Default for SharedGame {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            current_def: Mutex::new(None),
        }
    }
}

/// 当前是否有游戏正在占用输入。
pub fn is_game_active(app: &AppHandle) -> bool {
    let state: tauri::State<'_, SharedGame> = app.state();
    state.active.load(Ordering::SeqCst)
}

/// 启动内置 Snake 游戏，供 ActionBus 和 IPC 共用。
pub fn start_default_game(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_snake())
}

/// 启动指定游戏定义。
pub fn start_game(app: &AppHandle, def: GameDef) -> Result<(), String> {
    validate_game_def(&def)?;

    let state: tauri::State<'_, SharedGame> = app.state();
    {
        let mut current = state.current_def.lock().map_err(|e| e.to_string())?;
        *current = Some(def.clone());
    }
    state.active.store(true, Ordering::SeqCst);

    if let Err(e) = set_pet_state(app, PetStateName::GamePlay) {
        warn!(error = %e, "[game] 设置宠物 GamePlay 失败");
    }

    let window = match app.get_webview_window("game") {
        Some(w) => w,
        None => create_game_window(app)?,
    };

    position_fullscreen_on_monitor(app, &window);
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    info!(title = %def.title, "[game] 已启动");
    Ok(())
}

fn create_game_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    WebviewWindowBuilder::new(app, "game", WebviewUrl::App("game.html".into()))
        .title("8Bit Game")
        .inner_size(1280.0, 720.0)
        .decorations(false)
        .transparent(true)
        .background_color(tauri::webview::Color(0, 0, 0, 0))
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(true)
        .build()
        .map_err(|e| e.to_string())
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

fn set_pet_state(app: &AppHandle, state: PetStateName) -> Result<(), String> {
    let shared: tauri::State<'_, crate::commands::SharedPet> = app.state();
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    let status = crate::commands::set_state(&mut pet, state);
    app.emit(
        "pet-event",
        crate::gamepad::PetEvent::set_state(&status.state),
    )
    .map_err(|e| e.to_string())
}

/// 前端请求启动默认游戏。
#[tauri::command]
pub fn cmd_start_game(app: AppHandle) -> Result<(), String> {
    start_default_game(&app)
}

/// 前端或后续 AI 工具请求按配置启动游戏。
#[tauri::command]
pub fn cmd_start_game_with_def(app: AppHandle, def: GameDef) -> Result<(), String> {
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
    state.active.store(false, Ordering::SeqCst);
    if let Ok(mut current) = state.current_def.lock() {
        *current = None;
    }

    if let Some(w) = app.get_webview_window("game") {
        let _ = w.close();
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
