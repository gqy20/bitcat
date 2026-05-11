use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use ai_pad_core::pet::Pet;
use ai_pad_core::bridge::PetStateName;
use serde::{Deserialize, Serialize};

/// 共享宠物状态
pub struct SharedPet {
    pub pet: Mutex<Pet>,
    pub bubble: Mutex<Option<String>>,
}

impl SharedPet {
    pub fn new() -> Self {
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
}

impl SharedWindowState {
    pub fn new() -> Self {
        Self {
            collapsed: AtomicBool::new(false),
            always_on_top: AtomicBool::new(true),
            config_reload: AtomicBool::new(false),
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
pub fn cmd_set_state(shared: tauri::State<'_, SharedPet>, state: PetStateName) -> Result<PetStatus, String> {
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    Ok(set_state(&mut pet, state))
}

#[tauri::command]
pub fn cmd_walk_to(shared: tauri::State<'_, SharedPet>, x: f32) -> Result<PetStatus, String> {
    let mut pet = shared.pet.lock().map_err(|e| e.to_string())?;
    Ok(walk_to(&mut pet, x))
}

#[tauri::command]
pub fn cmd_show_bubble(shared: tauri::State<'_, SharedPet>, text: String) -> Result<PetStatus, String> {
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

// ---- 测试（TDD：先写测试，上面是实现） ----

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pet() -> Pet {
        Pet::new(64.0, 64.0)
    }

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
        // 手动推进帧
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
        // Idle → Idle 不重置
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
        assert_eq!(status.x, 64.0);
        assert_eq!(status.facing_right, true);
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
    use tauri::test::{mock_builder, noop_assets, mock_context, assert_ipc_response, get_ipc_response, MockRuntime};
    use tauri::{WebviewWindowBuilder, ipc::InvokeBody, ipc::CallbackFn, webview::InvokeRequest};
    use serde_json::json;

    fn build_test_app() -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
        let app = mock_builder()
            .manage(SharedPet::new())
            .invoke_handler(tauri::generate_handler![
                cmd_set_state,
                cmd_walk_to,
                cmd_show_bubble,
                cmd_get_status,
                cmd_tick,
            ])
            .build(mock_context(noop_assets()))
            .expect("failed to build mock app");

        let webview = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::App("index.html".into()))
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
        assert_ipc_response(&wv, req, Ok(json!({
            "state": "idle",
            "x": 64.0,
            "y": 64.0,
            "frame": 0,
            "facing_right": true,
            "bubble": null,
        })));
    }

    #[test]
    fn ipc_cmd_set_state_talk() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_set_state", json!({ "state": "Talk" }));
        assert_ipc_response(&wv, req, Ok(json!({
            "state": "talk",
            "x": 64.0,
            "y": 64.0,
            "frame": 0,
            "facing_right": true,
            "bubble": null,
        })));
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
        assert_ipc_response(&wv, req, Ok(json!({
            "state": "idle",
            "x": 64.0,
            "y": 64.0,
            "frame": 0,
            "facing_right": true,
            "bubble": "你好世界",
        })));
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
        assert_ipc_response(&wv, req2, Ok(json!({
            "state": "idle",
            "bubble": "persistent",
        })));
    }

    #[test]
    fn ipc_cmd_set_state_all_variants() {
        for state_name in &["Idle", "Walk", "Sleep", "Talk", "Happy", "Confused"] {
            let (_app, wv) = build_test_app();
            let req = invoke_request("cmd_set_state", json!({ "state": state_name }));
            let resp = get_ipc_response(&wv, req).unwrap();
            let status: PetStatus = resp.deserialize().unwrap();
            assert_eq!(
                status.state, state_name.to_lowercase(),
                "state mismatch for {state_name}"
            );
            assert_eq!(status.frame, 0, "frame should reset on state change");
        }
    }

    #[test]
    fn ipc_serialization_roundtrip_chinese() {
        let (_app, wv) = build_test_app();
        let req = invoke_request("cmd_show_bubble", json!({ "text": "喵喵喵 🐱" }));
        assert_ipc_response(&wv, req, Ok(json!({
            "bubble": "喵喵喵 🐱",
        })));
    }
}
