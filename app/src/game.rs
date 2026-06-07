//! 迷你游戏窗口与生命周期管理。
//!
//! 本模块负责创建全屏透明 game 窗口、保存当前 `GameDef`、接收前端结束回调，
//! 并把游戏结果同步回宠物状态。实际游戏逻辑运行在前端 `game_engine.js`，
//! core crate 只提供可序列化配置和参数边界。
use bitcat_core::bridge::PetStateName;
use bitcat_core::gomoku_ai::{GomokuAiMove, GomokuCommentary, GomokuPoint};
use bitcat_core::minigame::{validate_game_def, GameDef, MinigameType};
use bitcat_core::pet_event::{PetEvent, PetMode, PetMood};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};
#[cfg(windows)]
use windows_sys::Win32::Foundation::POINT;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Shared state for the game window lifecycle.
pub struct SharedGame {
    pub starting: AtomicBool,
    pub active: AtomicBool,
    pub startup_seq: std::sync::atomic::AtomicU64,
    pub current_def: Mutex<Option<GameDef>>,
    pub hidden_windows: Mutex<Vec<String>>,
}

/// Logical cursor position inside the transparent game window.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct GameCursorPosition {
    pub x: f64,
    pub y: f64,
}

/// Semantic battle event emitted by the frontend for pet reactions.
#[derive(Debug, Clone, Deserialize)]
pub struct BattlePetEventPayload {
    pub kind: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub damage: Option<i32>,
    #[serde(default)]
    pub hp_ratio: Option<f32>,
    #[serde(default)]
    pub interrupted: bool,
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

/// Return whether a game is currently capturing input.
pub fn is_game_active(app: &AppHandle) -> bool {
    let state: tauri::State<'_, SharedGame> = app.state();
    state.active.load(Ordering::SeqCst)
}

/// Return whether a game is starting or actively running.
pub fn is_game_busy(app: &AppHandle) -> bool {
    let state: tauri::State<'_, SharedGame> = app.state();
    state.starting.load(Ordering::SeqCst) || state.active.load(Ordering::SeqCst)
}

/// Return the current game lifecycle phase for logging and observation guards.
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

/// Start the built-in Snake mode.
pub fn start_default_game(app: &AppHandle) -> Result<(), String> {
    let mut def = GameDef::default_snake();
    let pack = bitcat_core::vocab::VocabPack::load_default()?;
    def.title = "单词贪吃蛇".into();
    def.dialogue.start = "吃掉正确释义".into();
    def.dialogue.win = "复习完成".into();
    def.dialogue.lose = "撞到了，先歇一下".into();
    def.rules.win_length = def
        .player
        .initial_length
        .saturating_add(pack.target_correct)
        .max(def.rules.win_length.min(def.player.initial_length + 12));
    def.snake_vocab = Some(pack.into_snake_config());
    start_game(app, def)
}

/// Start the built-in battle mode.
pub fn start_default_battle(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_battle())
}

/// Start the built-in memory matching mode.
pub fn start_default_memory(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_memory())
}

/// Start the built-in falling-catch mode.
pub fn start_default_catch(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_catch())
}

/// Start the built-in AI Gomoku mode.
pub fn start_default_gomoku(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_gomoku())
}

/// Start the built-in 3D fighting arena mode.
pub fn start_default_arena(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_arena())
}

/// Start the built-in pixel bead mode.
pub fn start_default_beads(app: &AppHandle) -> Result<(), String> {
    start_game(app, GameDef::default_beads())
}

/// Return the current game type for gamepad input routing.
pub fn current_game_type(app: &AppHandle) -> Option<MinigameType> {
    let state: tauri::State<'_, SharedGame> = app.state();
    state
        .current_def
        .lock()
        .ok()
        .and_then(|current| current.as_ref().map(|def| def.game_type))
}

/// Precreate the game window during app startup.
pub fn precreate_game_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("game").is_some() {
        return Ok(());
    }
    info!("[game] precreate window begin");
    create_game_window(app, 0)?;
    info!("[game] precreate window done");
    Ok(())
}

/// Start a game from a validated definition.
pub fn start_game(app: &AppHandle, def: GameDef) -> Result<(), String> {
    bitcat_core::points::award(
        bitcat_core::points::PointsEventKind::GamePlayed,
        Some(&def.title),
    );
    validate_game_def(&def)?;
    let overlay_mode = matches!(def.game_type, MinigameType::Battle);

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
                warn!(startup_id, title = %title, "[game] startup timed out");
            }
        });
    }

    info!(startup_id, title = %def.title, "[game] start requested");

    info!(startup_id, "[game] set pet state begin");
    if let Err(e) = set_pet_state(app, PetStateName::GamePlay) {
        warn!(error = %e, "[game] set pet GamePlay failed");
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
        configure_game_input_capture(&window, !overlay_mode);
        info!(startup_id, overlay_mode, "[game] enter window mode begin");
        let hidden_windows = hide_companion_windows(app, overlay_mode);
        if let Ok(mut stored) = state.hidden_windows.lock() {
            *stored = hidden_windows;
        }
        info!(startup_id, overlay_mode, "[game] enter window mode done");
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
        if let Some(window) = app.get_webview_window("game") {
            configure_game_input_capture(&window, true);
        }
        restore_companion_windows(app);
        let _ = set_pet_state(app, PetStateName::Idle);
        warn!(error = %e, "[game] start failed, state rolled back");
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
        .title("BitCat Game")
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
        warn!("[game] monitor unavailable, using current window size");
        return;
    };

    let pos = monitor.position();
    let size = monitor.size();
    let _ = window.set_position(PhysicalPosition::new(pos.x, pos.y));
    let _ = window.set_size(PhysicalSize::new(size.width, size.height));
}

fn configure_game_input_capture(window: &tauri::WebviewWindow, enabled: bool) {
    if let Err(e) = window.set_ignore_cursor_events(!enabled) {
        warn!(
            error = %e,
            enabled,
            "[game] set cursor capture failed"
        );
    }
}

fn hide_companion_windows(app: &AppHandle, overlay_mode: bool) -> Vec<String> {
    let mut hidden = Vec::new();
    let labels: &[&str] = if overlay_mode {
        &["bubble", "panel", "settings"]
    } else {
        &["pet", "pet-mini", "pet-snap", "bubble", "panel", "settings"]
    };
    for label in labels {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        if !window.is_visible().unwrap_or(false) {
            continue;
        }
        if let Err(e) = window.hide() {
            warn!(error = %e, label, "[game] hide companion window failed");
            continue;
        }
        hidden.push(label.to_string());
    }
    info!(hidden = ?hidden, overlay_mode, "[game] hidden companion windows");
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
                warn!(error = %e, label = %label, "[game] restore companion window failed");
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

/// Frontend request to start the built-in Snake mode.
#[tauri::command]
pub fn cmd_start_game(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_game invoked");
    start_default_game(&app)
}

/// Frontend request to start the built-in battle mode.
#[tauri::command]
pub fn cmd_start_battle(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_battle invoked");
    start_default_battle(&app)
}

/// Frontend request to start the built-in memory matching mode.
#[tauri::command]
pub fn cmd_start_memory(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_memory invoked");
    start_default_memory(&app)
}

/// Frontend request to start the built-in falling-catch mode.
#[tauri::command]
pub fn cmd_start_catch(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_catch invoked");
    start_default_catch(&app)
}

/// Frontend request to start the built-in AI Gomoku mode.
#[tauri::command]
pub fn cmd_start_gomoku(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_gomoku invoked");
    start_default_gomoku(&app)
}

/// Frontend request to start the built-in 3D fighting arena mode.
#[tauri::command]
pub fn cmd_start_arena(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_arena invoked");
    start_default_arena(&app)
}

/// Frontend request to start the built-in pixel bead mode.
#[tauri::command]
pub fn cmd_start_beads(app: AppHandle) -> Result<(), String> {
    info!("[game] cmd_start_beads invoked");
    start_default_beads(&app)
}

/// Frontend or AI request to start a game from a definition.
#[tauri::command]
pub fn cmd_start_game_with_def(app: AppHandle, def: GameDef) -> Result<(), String> {
    info!(title = %def.title, "[game] cmd_start_game_with_def invoked");
    start_game(&app, def)
}

/// Return the current game definition to the game window.
#[tauri::command]
pub fn cmd_get_current_game(shared: tauri::State<'_, SharedGame>) -> Result<GameDef, String> {
    let current = shared.current_def.lock().map_err(|e| e.to_string())?;
    current
        .clone()
        .ok_or_else(|| "褰撳墠娌℃湁娲诲姩娓告垙".to_string())
}

/// Toggle whether the game window captures mouse events.
#[tauri::command]
pub fn cmd_game_set_input_capture(app: AppHandle, enabled: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("game") else {
        return Ok(());
    };
    configure_game_input_capture(&window, enabled);
    Ok(())
}

/// Read the logical cursor position inside the game window.
#[tauri::command]
pub fn cmd_game_cursor_position(app: AppHandle) -> Result<GameCursorPosition, String> {
    let Some(window) = app.get_webview_window("game") else {
        return Err("game window not found".to_string());
    };
    let window_pos = window.outer_position().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let (cursor_x, cursor_y) = current_cursor_position_physical()?;
    Ok(GameCursorPosition {
        x: f64::from(cursor_x - window_pos.x) / scale,
        y: f64::from(cursor_y - window_pos.y) / scale,
    })
}

#[cfg(windows)]
fn current_cursor_position_physical() -> Result<(i32, i32), String> {
    let mut point = POINT { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut point) };
    if ok == 0 {
        Err("GetCursorPos failed".to_string())
    } else {
        Ok((point.x, point.y))
    }
}

#[cfg(not(windows))]
fn current_cursor_position_physical() -> Result<(i32, i32), String> {
    Err("game cursor polling is only supported on Windows".to_string())
}

/// Finish the game and restore hidden companion windows.
#[tauri::command]
pub fn cmd_game_end(app: AppHandle, result: String, score: u32) -> Result<(), String> {
    let normalized = result.trim().to_ascii_lowercase();
    if !matches!(normalized.as_str(), "win" | "lose" | "cancel") {
        return Err(format!("unknown game result: {result}"));
    }

    let state: tauri::State<'_, SharedGame> = app.state();
    state.starting.store(false, Ordering::SeqCst);
    state.active.store(false, Ordering::SeqCst);
    if let Ok(mut current) = state.current_def.lock() {
        *current = None;
    }

    if let Some(w) = app.get_webview_window("game") {
        configure_game_input_capture(&w, true);
        let _ = w.hide();
    }

    let pet_state = match normalized.as_str() {
        "win" => {
            bitcat_core::points::award(bitcat_core::points::PointsEventKind::GameWon, None);
            Some(PetStateName::GameWin)
        }
        "lose" => Some(PetStateName::GameLose),
        _ => Some(PetStateName::Idle),
    };
    if let Some(pet_state) = pet_state {
        if let Err(e) = set_pet_state(&app, pet_state) {
            warn!(error = %e, "[game] set final pet state failed");
        }
    }
    restore_companion_windows(&app);

    info!(result = %normalized, score, "[game] ended");
    Ok(())
}

/// Receive a low-frequency battle event for pet reactions.
#[tauri::command]
pub fn cmd_battle_pet_event(app: AppHandle, event: BattlePetEventPayload) -> Result<(), String> {
    let kind = normalized_battle_event_kind(&event);
    let pet_events = map_battle_pet_events(&event)?;
    let source = event_source_preview(&event);
    info!(kind = %kind, source = %source, "[battle] pet event");
    let bus: tauri::State<'_, crate::pet_event_bus::SharedPetEventBus> = app.state();
    for pet_event in pet_events {
        bus.emit(&app, pet_event);
    }
    Ok(())
}

fn normalized_battle_event_kind(event: &BattlePetEventPayload) -> String {
    event.kind.trim().to_ascii_lowercase()
}

fn map_battle_pet_events(event: &BattlePetEventPayload) -> Result<Vec<PetEvent>, String> {
    let mapped = match normalized_battle_event_kind(event).as_str() {
        "start" => vec![PetEvent::ShowBubble {
            text: "Battle started.".into(),
        }],
        "attack" => vec![PetEvent::React {
            mood: PetMood::Focused,
            speech: None,
            ttl_ms: Some(1_200),
        }],
        "skill" => vec![PetEvent::React {
            mood: PetMood::Excited,
            speech: None,
            ttl_ms: Some(1_500),
        }],
        "guard" => vec![PetEvent::React {
            mood: PetMood::Caring,
            speech: Some("Guarded.".into()),
            ttl_ms: Some(1_800),
        }],
        "interrupt" => vec![PetEvent::React {
            mood: PetMood::Excited,
            speech: Some("Interrupted.".into()),
            ttl_ms: Some(3_000),
        }],
        "pet_hit" => {
            let low_hp = event.hp_ratio.is_some_and(|hp| hp <= 0.35);
            vec![PetEvent::React {
                mood: PetMood::Confused,
                speech: low_hp.then(|| "Low HP.".into()),
                ttl_ms: Some(if low_hp { 3_000 } else { 1_400 }),
            }]
        }
        "win" => vec![PetEvent::React {
            mood: PetMood::Happy,
            speech: Some("Win!".into()),
            ttl_ms: Some(8_000),
        }],
        "lose" => vec![PetEvent::React {
            mood: PetMood::Confused,
            speech: Some("Try again.".into()),
            ttl_ms: Some(8_000),
        }],
        other => return Err(format!("unknown battle pet event: {other}")),
    };
    Ok(mapped)
}

fn event_source_preview(event: &BattlePetEventPayload) -> String {
    let mut parts = Vec::new();
    if let Some(source) = &event.source {
        parts.push(format!("source={source}"));
    }
    if let Some(skill_id) = &event.skill_id {
        parts.push(format!("skill={skill_id}"));
    }
    if let Some(damage) = event.damage {
        parts.push(format!("damage={damage}"));
    }
    if let Some(hp_ratio) = event.hp_ratio {
        parts.push(format!("hp_ratio={hp_ratio:.2}"));
    }
    if event.interrupted {
        parts.push("interrupted=true".to_string());
    }
    parts.join(" ")
}

/// Forward frontend game logs into Rust tracing.
#[tauri::command]
pub fn cmd_game_log(msg: String) -> Result<(), String> {
    if !bitcat_core::logging::frontend_log_allowed("game", std::time::Duration::from_millis(120)) {
        return Ok(());
    }
    let preview = bitcat_core::logging::log_preview(&msg, 120);
    info!(
        msg_chars = msg.chars().count(),
        msg_preview = %preview,
        "[game-js]"
    );
    Ok(())
}

/// Request one Gomoku response move from the configured AI model.
#[tauri::command]
pub async fn cmd_gomoku_ai_move(
    board: Vec<Vec<u8>>,
    last_move: Option<GomokuPoint>,
) -> Result<GomokuAiMove, String> {
    let ai_config = bitcat_core::ai_config::AiConfig::load()?;
    let mut last_error = None;
    for attempt in 1..=2 {
        match bitcat_core::gomoku_ai::choose_ai_move(&ai_config, &board, last_move).await {
            Ok(mv) => return Ok(mv),
            Err(e) => {
                warn!(
                    attempt,
                    error = %e,
                    "[gomoku] AI move extraction failed"
                );
                last_error = Some(e);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "gomoku ai move failed".to_string()))
}

/// Request a short Gomoku position commentary from the configured AI model.
#[tauri::command]
pub async fn cmd_gomoku_commentary(
    board: Vec<Vec<u8>>,
    last_move: Option<GomokuPoint>,
) -> Result<GomokuCommentary, String> {
    let ai_config = bitcat_core::ai_config::AiConfig::load()?;
    bitcat_core::gomoku_ai::comment_position(&ai_config, &board, last_move).await
}

/// Persist one completed Gomoku session for later replay and analysis.
#[tauri::command]
pub fn cmd_gomoku_record_game(record: Value) -> Result<(), String> {
    let dir = bitcat_core::storage::data_dir()?
        .join("games")
        .join("gomoku");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create gomoku log dir failed: {e}"))?;
    let path = dir.join("sessions.jsonl");
    let line = serde_json::to_string(&record).map_err(|e| format!("serialize gomoku log: {e}"))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open gomoku log {:?} failed: {e}", path))?;
    writeln!(file, "{line}").map_err(|e| format!("write gomoku log {:?} failed: {e}", path))?;
    info!(path = ?path, "[gomoku] session recorded");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{event_source_preview, map_battle_pet_events, BattlePetEventPayload};
    use bitcat_core::pet_event::{PetEvent, PetMood};

    #[test]
    fn battle_event_preview_keeps_structured_fields() {
        let event = BattlePetEventPayload {
            kind: "interrupt".to_string(),
            source: Some("skill".to_string()),
            skill_id: Some("scratch".to_string()),
            damage: Some(9),
            hp_ratio: Some(0.42),
            interrupted: true,
        };

        assert_eq!(
            event_source_preview(&event),
            "source=skill skill=scratch damage=9 hp_ratio=0.42 interrupted=true"
        );
    }

    #[test]
    fn battle_event_mapping_marks_low_pet_hp() {
        let event = BattlePetEventPayload {
            kind: "pet_hit".to_string(),
            source: Some("monster".to_string()),
            skill_id: None,
            damage: Some(4),
            hp_ratio: Some(0.25),
            interrupted: false,
        };

        assert_eq!(
            map_battle_pet_events(&event).unwrap(),
            vec![PetEvent::React {
                mood: PetMood::Confused,
                speech: Some("Low HP.".into()),
                ttl_ms: Some(3_000),
            }]
        );
    }

    #[test]
    fn battle_event_mapping_uses_excited_for_skill() {
        let event = BattlePetEventPayload {
            kind: "skill".to_string(),
            source: Some("skill".to_string()),
            skill_id: Some("scratch".to_string()),
            damage: Some(8),
            hp_ratio: Some(0.8),
            interrupted: false,
        };

        assert_eq!(
            map_battle_pet_events(&event).unwrap(),
            vec![PetEvent::React {
                mood: PetMood::Excited,
                speech: None,
                ttl_ms: Some(1_500),
            }]
        );
    }
}
