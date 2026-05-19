use ai_pad_core::bridge::PetStateName;
use ai_pad_core::pet::Pet;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tracing::info;

const SNAP_SIDE_BOTTOM_GAP: i32 = 14;

/// 共享宠物状态
pub struct SharedPet {
    pub pet: Mutex<Pet>,
    pub bubble: Mutex<Option<String>>,
}

impl Default for SharedPet {
    fn default() -> Self {
        Self {
            pet: Mutex::new(Pet::new(64.0, 64.0)),
            bubble: Mutex::new(None),
        }
    }
}

/// 窗口级共享状态（托盘/手柄线程安全并发读写）
pub struct SharedWindowState {
    pub collapsed: AtomicBool,
    pub always_on_top: AtomicBool,
    pub config_reload: AtomicBool,
    /// 最后已知窗口位置：折叠时旧窗口已销毁，展开重建需要坐标
    pub last_position: Mutex<Option<(i32, i32)>>,
    /// 贴边吸附状态
    pub is_snapped: Mutex<bool>,
    pub snap_edge: Mutex<Option<String>>,
}

impl Default for SharedWindowState {
    fn default() -> Self {
        Self {
            collapsed: AtomicBool::new(false),
            always_on_top: AtomicBool::new(true),
            config_reload: AtomicBool::new(false),
            last_position: Mutex::new(None),
            is_snapped: Mutex::new(false),
            snap_edge: Mutex::new(None),
        }
    }
}

/// 宠物状态快照，返回给前端
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PetStatus {
    pub state: String,
    pub x: f32,
    pub y: f32,
    pub frame: usize,
    pub facing_right: bool,
    pub bubble: Option<String>,
}

// ---- 纯逻辑函数（可脱离 Tauri 独立测试）----

pub fn set_state(pet: &mut Pet, state: PetStateName) -> PetStatus {
    let ps: ai_pad_core::pet::PetState = state.into();
    pet.set_state(ps);
    snapshot(pet, &None)
}

pub fn walk_to(pet: &mut Pet, x: f32) -> PetStatus {
    pet.walk_to(x);
    snapshot(pet, &None)
}

pub fn show_bubble(pet: &Pet, bubble: &mut Option<String>, text: String) -> PetStatus {
    *bubble = Some(text);
    snapshot(pet, bubble)
}

pub fn get_status(pet: &Pet, bubble: &Option<String>) -> PetStatus {
    snapshot(pet, bubble)
}

pub fn tick(pet: &mut Pet, dt_ms: u64) -> PetStatus {
    pet.update(dt_ms);
    snapshot(pet, &None)
}

fn snapshot(pet: &Pet, bubble: &Option<String>) -> PetStatus {
    PetStatus {
        state: format!("{:?}", pet.state).to_lowercase(),
        x: pet.x,
        y: pet.y,
        frame: pet.frame,
        facing_right: pet.facing_right,
        bubble: bubble.clone(),
    }
}

// ---- Tauri command 包装 ----

#[tauri::command]
pub fn cmd_set_state(
    shared: tauri::State<'_, SharedPet>,
    state: PetStateName,
) -> Result<PetStatus, String> {
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    Ok(set_state(&mut pet, state))
}

#[tauri::command]
pub fn cmd_walk_to(shared: tauri::State<'_, SharedPet>, x: f32) -> Result<PetStatus, String> {
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    Ok(walk_to(&mut pet, x))
}

#[tauri::command]
pub fn cmd_show_bubble(
    shared: tauri::State<'_, SharedPet>,
    text: String,
) -> Result<PetStatus, String> {
    let pet = shared.pet.lock().map_err(|e| e.to_string())?;
    let mut bubble = shared.bubble.lock().map_err(|e| e.to_string())?;
    Ok(show_bubble(&pet, &mut bubble, text))
}

#[tauri::command]
pub fn cmd_get_status(shared: tauri::State<'_, SharedPet>) -> Result<PetStatus, String> {
    let pet = shared.pet.lock().map_err(|e| e.to_string())?;
    let bubble = shared.bubble.lock().map_err(|e| e.to_string())?;
    Ok(get_status(&pet, &bubble))
}

/// 前端表现播放器结束时回报，用于精确复位后端表现状态。
#[tauri::command]
pub fn cmd_performance_finished(session_id: u64, reason: Option<String>) -> Result<(), String> {
    ai_pad_core::performance::stop_performance(session_id, reason.as_deref().unwrap_or("finished"));
    info!(
        session_id,
        reason = reason.as_deref().unwrap_or("finished"),
        "[cmd] 表现会话结束"
    );
    Ok(())
}

#[tauri::command]
pub fn cmd_tick(shared: tauri::State<'_, SharedPet>, dt_ms: u64) -> Result<PetStatus, String> {
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    Ok(tick(&mut pet, dt_ms))
}

/// 播放舞蹈：通过 ActionBus 归一分发，底层复用 dance-bridge 管线
#[tauri::command]
pub async fn cmd_play_dance(
    shared: tauri::State<'_, SharedPet>,
    dance_name: String,
    app: tauri::AppHandle,
) -> Result<PetStatus, String> {
    info!(dance = %dance_name, "[cmd] 播放舞蹈请求");

    crate::action_bus::ActionBus::dispatch(
        &app,
        crate::action_bus::Action::PlayDance(dance_name),
        crate::action_bus::ActionSource::Frontend {
            cmd: "cmd_play_dance".into(),
        },
    );

    // 返回当前 pet 状态（舞蹈播放由前端接管渲染）
    let pet = shared.pet.lock().map_err(|e| e.to_string())?;
    let bubble = shared.bubble.lock().map_err(|e| e.to_string())?;
    Ok(get_status(&pet, &bubble))
}

/// 窗口状态快照，返回给前端（pull 模式，替代不可靠的 emit push）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowStateSnapshot {
    pub collapsed: bool,
    pub always_on_top: bool,
    pub position: Option<(i32, i32)>,
    /// 吸附方向: "left" / "right" / None（未吸附）
    /// 前端 init 时 pull 读取此字段决定 edgeReversed（替代 eval 注入 __setSnapEdge）
    pub snap_edge: Option<String>,
    pub snap_w: f64,
    pub snap_h: f64,
}

/// 从 SharedWindowState 读取当前快照（纯函数，可单测）
pub fn window_state_snapshot(ws: &SharedWindowState) -> WindowStateSnapshot {
    WindowStateSnapshot {
        collapsed: ws.collapsed.load(Ordering::SeqCst),
        always_on_top: ws.always_on_top.load(Ordering::SeqCst),
        position: *ws.last_position.lock().unwrap(),
        snap_edge: ws.snap_edge.lock().ok().and_then(|g| g.clone()),
        snap_w: crate::snap::SNAP_W,
        snap_h: crate::snap::SNAP_H as f64,
    }
}

/// 前端 init 时 pull 拉取窗口状态（替代不可靠的 emit push 时序）
#[tauri::command]
pub fn cmd_get_window_state(
    state: tauri::State<'_, SharedWindowState>,
) -> Result<WindowStateSnapshot, String> {
    Ok(window_state_snapshot(&state))
}

// ========================================================================
// Task 5: 磁性预告（Snap Preview）
//
// 当宠物被拖拽靠近屏幕左右边缘时，应显示一个"预告条"提示松手会吸附到此。
// 本模块只负责判定逻辑（纯函数 + Tauri command 包装），UI 侧的预告窗口
// 由前端结合 cmd_snap_transform 复用。
// ========================================================================

/// 磁性预告结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapPreview {
    pub edge: String, // "left" | "right" | "top" | "bottom" | "none"
    pub x: i32,       // 预告条左上角 X（物理像素）
    pub y: i32,       // 预告条左上角 Y（物理像素）
    pub visible: bool,
}

/// 计算磁性预告条的位置。
///
/// - `cursor_x`  : 当前宠物窗口左上角 X（物理像素）
/// - `work_left` / `work_right` : 工作区左右边界
/// - `work_bottom` : 工作区底部（预告条贴底显示）
/// - `pet_w` : 当前宠物窗口物理宽度
/// - `snap_w` / `snap_h` : 预告条尺寸
/// - `threshold` : 触发阈值（物理像素）
///
/// 返回 `SnapPreview { visible:false, edge:"none" }` 表示当前无需预告。
#[allow(clippy::too_many_arguments)]
pub fn calc_snap_preview(
    cursor_x: i32,
    cursor_y: i32,
    work_left: i32,
    work_top: i32,
    work_right: i32,
    work_bottom: i32,
    pet_w: i32,
    pet_h: i32,
    snap_w: i32,
    snap_h: i32,
    threshold: i32,
) -> SnapPreview {
    let left_dist = (cursor_x - work_left).max(0);
    let right_dist = (work_right - pet_w - cursor_x).max(0);
    let top_dist = (cursor_y - work_top).max(0);
    let bottom_dist = (work_bottom - pet_h - cursor_y).max(0);

    let candidates = [
        ("left", left_dist),
        ("right", right_dist),
        ("top", top_dist),
        ("bottom", bottom_dist),
    ];

    let (edge, dist) = candidates
        .iter()
        .min_by_key(|(_, dist)| *dist)
        .copied()
        .unwrap_or(("none", threshold + 1));

    if dist > threshold {
        return SnapPreview {
            edge: "none".to_string(),
            x: cursor_x,
            y: cursor_y,
            visible: false,
        };
    }

    let side_snap_y = work_bottom - snap_h - SNAP_SIDE_BOTTOM_GAP;

    match edge {
        "left" => SnapPreview {
            edge: "left".to_string(),
            x: work_left,
            y: side_snap_y.clamp(work_top, work_bottom - snap_h),
            visible: true,
        },
        "right" => SnapPreview {
            edge: "right".to_string(),
            x: work_right - snap_w,
            y: side_snap_y.clamp(work_top, work_bottom - snap_h),
            visible: true,
        },
        "top" => SnapPreview {
            edge: "top".to_string(),
            x: cursor_x.clamp(work_left, work_right - snap_h),
            y: work_top,
            visible: true,
        },
        "bottom" => SnapPreview {
            edge: "bottom".to_string(),
            x: cursor_x.clamp(work_left, work_right - snap_h),
            y: work_bottom - snap_w,
            visible: true,
        },
        _ => SnapPreview {
            edge: "none".to_string(),
            x: cursor_x,
            y: cursor_y,
            visible: false,
        },
    }
}

// ---- 测试（TDD：先写测试，上面是实现） ----

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn test_pet() -> Pet {
        Pet::new(64.0, 64.0)
    }

    // ===== SharedWindowState 测试 =====

    #[test]
    fn test_window_state_defaults() {
        let ws = SharedWindowState::default();
        assert!(!ws.collapsed.load(Ordering::SeqCst));
        assert!(ws.always_on_top.load(Ordering::SeqCst));
        assert!(!ws.config_reload.load(Ordering::SeqCst));
    }

    #[test]
    fn test_last_position_default_none() {
        let ws = SharedWindowState::default();
        let pos = ws.last_position.lock().unwrap();
        assert!(pos.is_none(), "last_position 应默认为 None");
    }

    #[test]
    fn test_set_and_get_last_position() {
        let ws = SharedWindowState::default();
        {
            let mut pos = ws.last_position.lock().unwrap();
            *pos = Some((100, 200));
        }
        let pos = ws.last_position.lock().unwrap();
        assert_eq!(*pos, Some((100, 200)));
    }

    #[test]
    fn test_update_last_position_overwrites() {
        let ws = SharedWindowState::default();
        {
            let mut pos = ws.last_position.lock().unwrap();
            *pos = Some((10, 20));
        }
        {
            let mut pos = ws.last_position.lock().unwrap();
            *pos = Some((300, 400));
        }
        let pos = ws.last_position.lock().unwrap();
        assert_eq!(*pos, Some((300, 400)));
    }

    #[test]
    fn test_collapse_toggle_atomic() {
        let ws = SharedWindowState::default();
        assert!(!ws.collapsed.load(Ordering::SeqCst));
        ws.collapsed.store(true, Ordering::SeqCst);
        assert!(ws.collapsed.load(Ordering::SeqCst));
        ws.collapsed.store(false, Ordering::SeqCst);
        assert!(!ws.collapsed.load(Ordering::SeqCst));
    }

    #[test]
    fn test_always_on_top_toggle_atomic() {
        let ws = SharedWindowState::default();
        assert!(ws.always_on_top.load(Ordering::SeqCst));
        ws.always_on_top.store(false, Ordering::SeqCst);
        assert!(!ws.always_on_top.load(Ordering::SeqCst));
    }

    // ===== WindowStateSnapshot 测试 =====

    #[test]
    fn test_snapshot_default_state() {
        let ws = SharedWindowState::default();
        let snap = window_state_snapshot(&ws);
        assert_eq!(
            snap,
            WindowStateSnapshot {
                collapsed: false,
                always_on_top: true,
                position: None,
                snap_edge: None,
                snap_w: crate::snap::SNAP_W,
                snap_h: crate::snap::SNAP_H as f64,
            }
        );
    }

    #[test]
    fn test_snapshot_after_collapse() {
        let ws = SharedWindowState::default();
        ws.collapsed.store(true, Ordering::SeqCst);
        let snap = window_state_snapshot(&ws);
        assert!(snap.collapsed);
        assert!(snap.always_on_top);
    }

    #[test]
    fn test_snapshot_with_position() {
        let ws = SharedWindowState::default();
        *ws.last_position.lock().unwrap() = Some((500, 300));
        let snap = window_state_snapshot(&ws);
        assert_eq!(snap.position, Some((500, 300)));
    }

    #[test]
    fn test_snapshot_serialization_roundtrip() {
        let snap = WindowStateSnapshot {
            collapsed: true,
            always_on_top: false,
            position: Some((1920, 1080)),
            snap_edge: None,
            snap_w: crate::snap::SNAP_W,
            snap_h: crate::snap::SNAP_H as f64,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: WindowStateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, snap);
    }

    #[test]
    fn test_snapshot_serialization_null_position() {
        let snap = WindowStateSnapshot {
            collapsed: false,
            always_on_top: true,
            position: None,
            snap_edge: None,
            snap_w: crate::snap::SNAP_W,
            snap_h: crate::snap::SNAP_H as f64,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["position"].is_null());
    }

    // ===== snap_edge Pull 模式测试（Task 2）=====

    #[test]
    fn test_snap_edge_default_none() {
        let ws = SharedWindowState::default();
        let edge = ws.snap_edge.lock().unwrap();
        assert!(edge.is_none(), "snap_edge 默认应为 None");
    }

    #[test]
    fn test_set_snap_edge_left_then_right() {
        let ws = SharedWindowState::default();
        *ws.snap_edge.lock().unwrap() = Some("left".to_string());
        assert_eq!(ws.snap_edge.lock().unwrap().as_deref(), Some("left"));
        *ws.snap_edge.lock().unwrap() = Some("right".to_string());
        assert_eq!(ws.snap_edge.lock().unwrap().as_deref(), Some("right"));
    }

    #[test]
    fn test_snapshot_includes_snap_edge_default_none() {
        let ws = SharedWindowState::default();
        let snap = window_state_snapshot(&ws);
        assert_eq!(snap.snap_edge, None, "默认快照 snap_edge 应为 None");
    }

    #[test]
    fn test_snapshot_includes_snap_edge_left() {
        let ws = SharedWindowState::default();
        *ws.snap_edge.lock().unwrap() = Some("left".to_string());
        let snap = window_state_snapshot(&ws);
        assert_eq!(snap.snap_edge.as_deref(), Some("left"));
    }

    #[test]
    fn test_snapshot_includes_snap_edge_right() {
        let ws = SharedWindowState::default();
        *ws.snap_edge.lock().unwrap() = Some("right".to_string());
        let snap = window_state_snapshot(&ws);
        assert_eq!(snap.snap_edge.as_deref(), Some("right"));
    }

    #[test]
    fn test_snapshot_unsnap_clears_snap_edge() {
        let ws = SharedWindowState::default();
        *ws.snap_edge.lock().unwrap() = Some("left".to_string());
        // 模拟 unsnap
        *ws.snap_edge.lock().unwrap() = None;
        let snap = window_state_snapshot(&ws);
        assert_eq!(snap.snap_edge, None);
    }

    #[test]
    fn test_snapshot_serialization_includes_snap_edge() {
        let snap = WindowStateSnapshot {
            collapsed: false,
            always_on_top: true,
            position: None,
            snap_edge: Some("right".to_string()),
            snap_w: crate::snap::SNAP_W,
            snap_h: crate::snap::SNAP_H as f64,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["snap_edge"], serde_json::json!("right"));
    }

    #[test]
    fn test_snapshot_serialization_null_snap_edge() {
        let snap = WindowStateSnapshot {
            collapsed: false,
            always_on_top: true,
            position: None,
            snap_edge: None,
            snap_w: crate::snap::SNAP_W,
            snap_h: crate::snap::SNAP_H as f64,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["snap_edge"].is_null());
    }

    #[test]
    fn test_snapshot_roundtrip_with_snap_edge() {
        let snap = WindowStateSnapshot {
            collapsed: true,
            always_on_top: false,
            position: Some((100, 200)),
            snap_edge: Some("left".to_string()),
            snap_w: crate::snap::SNAP_W,
            snap_h: crate::snap::SNAP_H as f64,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: WindowStateSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, snap);
    }

    // ===== 磁性预告 calc_snap_preview（Task 5）=====

    fn preview_at(x: i32, y: i32) -> SnapPreview {
        // 典型工作区 1920x1080，宠物 128x128，阈值 20，snap 24x67
        calc_snap_preview(x, y, 0, 0, 1920, 1040, 128, 128, 24, 67, 20)
    }

    #[test]
    fn test_preview_left_edge_triggers() {
        let p = preview_at(10, 500);
        assert_eq!(p.edge, "left");
        assert!(p.visible);
        assert_eq!(p.x, 0);
        assert_eq!(p.y, 1040 - 67 - SNAP_SIDE_BOTTOM_GAP);
    }

    #[test]
    fn test_preview_right_edge_triggers() {
        // 1920 - 128 - 10 = 1782，距右 10px
        let p = preview_at(1782, 500);
        assert_eq!(p.edge, "right");
        assert!(p.visible);
        assert_eq!(p.x, 1920 - 24);
        assert_eq!(p.y, 1040 - 67 - SNAP_SIDE_BOTTOM_GAP);
    }

    #[test]
    fn test_preview_top_edge_triggers() {
        let p = preview_at(900, 10);
        assert_eq!(p.edge, "top");
        assert!(p.visible);
        assert_eq!(p.x, 900);
        assert_eq!(p.y, 0);
    }

    #[test]
    fn test_preview_bottom_edge_triggers() {
        let p = preview_at(900, 902);
        assert_eq!(p.edge, "bottom");
        assert!(p.visible);
        assert_eq!(p.x, 900);
        assert_eq!(p.y, 1040 - 24);
    }

    #[test]
    fn test_preview_center_no_trigger() {
        let p = preview_at(900, 500);
        assert_eq!(p.edge, "none");
        assert!(!p.visible);
    }

    #[test]
    fn test_preview_left_boundary_exact_threshold() {
        // x=80 → left_dist=80，等于阈值 → 应触发
        let p = preview_at(80, 500);
        assert_eq!(p.edge, "left");
        assert!(p.visible);
    }

    #[test]
    fn test_preview_left_boundary_just_outside_threshold() {
        // x=81 → left_dist=81 > 80 → 不触发（若 right 也远则 none）
        let p = preview_at(81, 500);
        assert_eq!(p.edge, "none");
        assert!(!p.visible);
    }

    #[test]
    fn test_preview_prefers_closer_edge() {
        // 距左 60px、距右 (1920-128-60)=1732 → 偏左
        let p = preview_at(60, 500);
        assert_eq!(p.edge, "left");
    }

    #[test]
    fn test_preview_right_when_left_far() {
        // x=1790 → left_dist=1790, right_dist=1920-128-1790=2
        let p = preview_at(1790, 500);
        assert_eq!(p.edge, "right");
    }

    #[test]
    fn test_preview_prefers_closest_vertical_edge() {
        let p = preview_at(60, 10);
        assert_eq!(p.edge, "top");
    }

    #[test]
    fn test_preview_negative_cursor_clamped() {
        // x 越界到负值（窗口被拖出屏幕外）：left_dist 被 max(0) 钳制为 0，应触发 left
        let p = preview_at(-50, 500);
        assert_eq!(p.edge, "left");
        assert!(p.visible);
    }

    #[test]
    fn test_preview_cursor_beyond_right_clamped() {
        // x=2000 超过工作区：right_dist=(1920-128-2000).max(0)=0，触发 right
        let p = preview_at(2000, 500);
        assert_eq!(p.edge, "right");
        assert!(p.visible);
    }

    #[test]
    fn test_preview_preview_serialization() {
        let p = SnapPreview {
            edge: "left".to_string(),
            x: 0,
            y: 940,
            visible: true,
        };
        let json = serde_json::to_string(&p).unwrap();
        let restored: SnapPreview = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, p);
    }

    // ===== Pet 状态机测试（原有） =====

    #[test]
    fn test_set_state_idle_to_talk() {
        let mut pet = test_pet();
        assert_eq!(pet.state, ai_pad_core::pet::PetState::Idle);

        let status = set_state(&mut pet, PetStateName::Talk);
        assert_eq!(status.state, "talk");
        assert_eq!(pet.state, ai_pad_core::pet::PetState::Talk);
    }

    #[test]
    fn test_set_state_resets_frame() {
        let mut pet = test_pet();
        pet.frame = 3;
        pet.frame_time_ms = 999;
        pet.state_time_ms = 9999;

        set_state(&mut pet, PetStateName::Talk);

        assert_eq!(pet.frame, 0);
        assert_eq!(pet.frame_time_ms, 0);
        assert_eq!(pet.state_time_ms, 0);
    }

    #[test]
    fn test_set_same_state_no_change() {
        let mut pet = test_pet();
        pet.frame_time_ms = 500;
        set_state(&mut pet, PetStateName::Idle);
        assert_eq!(pet.frame_time_ms, 500);
    }

    #[test]
    fn test_walk_to_updates_target() {
        let mut pet = test_pet();
        let status = walk_to(&mut pet, 200.0);
        assert_eq!(pet.state, ai_pad_core::pet::PetState::Walk);
        assert_eq!(pet.target_x, Some(200.0));
        assert_eq!(status.state, "walk");
    }

    #[test]
    fn test_show_bubble_stores_text() {
        let pet = test_pet();
        let mut bubble = None;
        let status = show_bubble(&pet, &mut bubble, "喵~".into());
        assert_eq!(status.bubble, Some("喵~".to_string()));
        assert_eq!(bubble, Some("喵~".to_string()));
    }

    #[test]
    fn test_get_status_returns_current() {
        let mut pet = test_pet();
        pet.frame = 2;
        let bubble = Some("test".into());
        let status = get_status(&pet, &bubble);
        assert_eq!(status.state, "idle");
        assert_eq!(status.frame, 2);
        assert_eq!(pet.x, 64.0);
        assert_eq!(pet.facing_right, true);
        assert_eq!(status.bubble, Some("test".to_string()));
    }

    #[test]
    fn test_tick_advances_frame() {
        let mut pet = test_pet();
        assert_eq!(pet.frame, 0);

        tick(&mut pet, 1499);
        assert_eq!(pet.frame, 0);

        tick(&mut pet, 1);
        assert_eq!(pet.frame, 1);
    }

    #[test]
    fn test_tick_walk_moves() {
        let mut pet = Pet::new(0.0, 0.0);
        pet.speed = 100.0;
        pet.walk_to(50.0);

        tick(&mut pet, 500);
        assert!((pet.x - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_snapshot_state_names() {
        let mut pet = test_pet();
        for (name, state) in [
            ("idle", PetStateName::Idle),
            ("walk", PetStateName::Walk),
            ("sleep", PetStateName::Sleep),
            ("talk", PetStateName::Talk),
            ("happy", PetStateName::Happy),
            ("confused", PetStateName::Confused),
        ] {
            set_state(&mut pet, state);
            assert_eq!(snapshot(&pet, &None).state, name);
        }
    }

    #[test]
    fn test_auto_idle_timeout() {
        let mut pet = test_pet();
        set_state(&mut pet, PetStateName::Walk);
        assert_eq!(pet.state, ai_pad_core::pet::PetState::Walk);

        tick(&mut pet, 3000);
        assert_eq!(pet.state, ai_pad_core::pet::PetState::Idle);
    }

    // ===== 舞蹈命令测试 =====

    #[test]
    fn test_play_dance_serializes_dance_def_for_emit() {
        // 验证 DanceDef 可以被序列化为 emit payload
        let def = ai_pad_core::dance::DanceDef {
            name: "test".into(),
            loop_: true,
            steps: vec![ai_pad_core::dance::DanceStep {
                action: ai_pad_core::dance::DanceAction::Jump,
                duration_ms: 300,
                repeat: 1,
            }],
        };
        let payload = serde_json::to_value(&def).unwrap();
        assert_eq!(payload["name"], "test");
        assert!(payload["loop_"].as_bool().unwrap());
        assert_eq!(payload["steps"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_play_dance_unknown_name_returns_err() {
        // 不存在的舞蹈名应返回错误
        let result = ai_pad_core::dance::load_dance("nonexistent_dance_xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_performance_finished_resets_performing_state() {
        let session = ai_pad_core::performance::start_performance(
            ai_pad_core::performance::PerformanceKind::ChoreographedDance,
        );
        cmd_performance_finished(session.id, Some("finished".into())).unwrap();
        assert!(!ai_pad_core::performance::is_performing());
    }
}

// ---- Tauri IPC 集成测试 (Mock Runtime) ----
// 测试命令通过 IPC 层的序列化/反序列化和 State 提取
//
// 运行: cargo test -p ai-pad-app --features ipc-tests -- commands::ipc_tests
// 需要: WebView2 Evergreen Runtime (CI 已预装，本地可能需要手动安装)
// Windows 本地缺少 WebView2 时会报 STATUS_ENTRYPOINT_NOT_FOUND

#[cfg(all(test, feature = "ipc-tests"))]
mod ipc_tests {
    use super::*;
    use serde_json::json;
    use tauri::test::{
        assert_ipc_response, get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime,
    };
    use tauri::{ipc::CallbackFn, ipc::InvokeBody, webview::InvokeRequest, WebviewWindowBuilder};

    fn build_test_app() -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
        let app = mock_builder()
            .manage(SharedPet::default())
            .invoke_handler(tauri::generate_handler![
                cmd_set_state,
                cmd_walk_to,
                cmd_show_bubble,
                cmd_get_status,
                cmd_tick,
            ])
            .build(mock_context(noop_assets()))
            .expect("failed to build mock app");

        let webview =
            WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::App("index.html".into()))
                .build()
                .expect("failed to build mock webview");

        (app, webview)
    }

    fn invoke_request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::from(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        }
    }

    #[test]
    fn ipc_cmd_get_status_returns_initial() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_get_status", json!({}));
        assert_ipc_response(
            &wv,
            req,
            Ok(json!({
                "state": "idle",
                "x": 64.0,
                "y": 64.0,
                "frame": 0,
                "facing_right": true,
                "bubble": null,
            })),
        );
    }

    #[test]
    fn ipc_cmd_set_state_talk() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_set_state", json!({ "state": "Talk" }));
        assert_ipc_response(
            &wv,
            req,
            Ok(json!({
                "state": "talk",
                "x": 64.0,
                "y": 64.0,
                "frame": 0,
                "facing_right": true,
                "bubble": null,
            })),
        );
    }

    #[test]
    fn ipc_cmd_walk_to() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_walk_to", json!({ "x": 200.0 }));
        let resp = get_ipc_response(&wv, req).expect("ipc response ok");
        let status: PetStatus = resp.deserialize().expect("deserialize PetStatus");
        assert_eq!(status.state, "walk");
        assert!((status.x - 64.0).abs() < 1.0);
    }

    #[test]
    fn ipc_cmd_show_bubble() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_show_bubble", json!({ "text": "你好世界" }));
        assert_ipc_response(
            &wv,
            req,
            Ok(json!({
                "state": "idle",
                "x": 64.0,
                "y": 64.0,
                "frame": 0,
                "facing_right": true,
                "bubble": "你好世界",
            })),
        );
    }

    #[test]
    fn ipc_cmd_tick_advances_frame() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_tick", json!({ "dt_ms": 500 }));
        let resp = get_ipc_response(&wv, req).expect("ipc response ok");
        let status: PetStatus = resp.deserialize().expect("deserialize PetStatus");
        assert_eq!(status.frame, 1);
    }

    #[test]
    fn ipc_multiple_commands_share_state() {
        let (_app, wv) = build_test_app();
        let req1 = invoke_request("cmd_show_bubble", json!({ "text": "persistent" }));
        get_ipc_response(&wv, req1).expect("show_bubble ok");

        let req2 = invoke_request("cmd_get_status", json!({}));
        assert_ipc_response(
            &wv,
            req2,
            Ok(json!({
                "state": "idle",
                "bubble": "persistent",
            })),
        );
    }

    #[test]
    fn ipc_cmd_set_state_all_variants() {
        for state_name in &["Idle", "Walk", "Sleep", "Talk", "Happy", "Confused"] {
            let (_app, wv) = build_test_app();
            let req = invoke_request("cmd_set_state", json!({ "state": state_name }));
            let resp = get_ipc_response(&wv, req).unwrap();
            let status: PetStatus = resp.deserialize().unwrap();
            assert_eq!(
                status.state,
                state_name.to_lowercase(),
                "state mismatch for {state_name}"
            );
            assert_eq!(status.frame, 0, "frame should reset on state change");
        }
    }

    #[test]
    fn ipc_serialization_roundtrip_chinese() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_show_bubble", json!({ "text": "喵喵喵 🐱" }));
        assert_ipc_response(
            &wv,
            req,
            Ok(json!({
                "bubble": "喵喵喵 🐱",
            })),
        );
    }
}
