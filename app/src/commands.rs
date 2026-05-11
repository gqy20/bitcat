use ai_pad_core::bridge::PetStateName;
use ai_pad_core::pet::Pet;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

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

#[tauri::command]
pub fn cmd_tick(shared: tauri::State<'_, SharedPet>, dt_ms: u64) -> Result<PetStatus, String> {
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    Ok(tick(&mut pet, dt_ms))
}

/// 窗口状态快照，返回给前端（pull 模式，替代不可靠的 emit push）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowStateSnapshot {
    pub collapsed: bool,
    pub always_on_top: bool,
    pub position: Option<(i32, i32)>,
}

/// 从 SharedWindowState 读取当前快照（纯函数，可单测）
pub fn window_state_snapshot(ws: &SharedWindowState) -> WindowStateSnapshot {
    WindowStateSnapshot {
        collapsed: ws.collapsed.load(Ordering::SeqCst),
        always_on_top: ws.always_on_top.load(Ordering::SeqCst),
        position: *ws.last_position.lock().unwrap(),
    }
}

/// 前端 init 时 pull 拉取窗口状态（替代不可靠的 emit push 时序）
#[tauri::command]
pub fn cmd_get_window_state(
    state: tauri::State<'_, SharedWindowState>,
) -> Result<WindowStateSnapshot, String> {
    Ok(window_state_snapshot(&state))
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
        };
        let json = serde_json::to_string(&snap).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["position"].is_null());
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

        tick(&mut pet, 499);
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
