use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder};

const BUBBLE_W: f64 = 280.0;
const BUBBLE_H: f64 = 140.0;

/// 后端待消费文本：首次创建窗口时 emit 时机早于前端 listen 注册，
/// 因此把文本存这里，前端 init 时主动 invoke 拉一次。
pub struct SharedBubble {
    pub pending_text: Mutex<Option<String>>,
}

impl SharedBubble {
    pub fn new() -> Self {
        Self {
            pending_text: Mutex::new(None),
        }
    }
}

impl Default for SharedBubble {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Serialize, Clone)]
struct BubblePayload {
    text: String,
}

#[derive(serde::Serialize, Clone)]
struct BubbleChunkPayload {
    chunk: String,
}

/// 显示气泡：按需创建窗口、定位到 pet 上方、写入待消费文本 + emit
pub fn show_bubble(app: &AppHandle, text: &str) -> Result<(), String> {
    // 写入 pending text
    let state: State<SharedBubble> = app.state();
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(text.to_string());

    // 取或创建窗口
    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };

    // 定位到 pet 上方
    position_above_pet(app, &window);

    let _ = window.show();
    // emit 兜底：窗口已存在时 listener 已注册，立刻刷新
    let _ = app.emit_to(
        "bubble",
        "bubble-update",
        BubblePayload {
            text: text.to_string(),
        },
    );

    Ok(())
}

/// 流式开始: 清空 pending、确保窗口显示、emit "bubble-start"
pub fn start_streaming_bubble(app: &AppHandle) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    *state.pending_text.lock().map_err(|e| e.to_string())? = Some(String::new());

    let window = match app.get_webview_window("bubble") {
        Some(w) => w,
        None => create_bubble_window(app).map_err(|e| e.to_string())?,
    };
    position_above_pet(app, &window);
    let _ = window.show();
    let _ = app.emit_to("bubble", "bubble-start", ());
    Ok(())
}

/// 流式追加: 累加到 pending(给晚到的 listener 用)、emit "bubble-chunk"
pub fn append_bubble_chunk(app: &AppHandle, chunk: &str) -> Result<(), String> {
    let state: State<SharedBubble> = app.state();
    if let Ok(mut g) = state.pending_text.lock() {
        if let Some(s) = g.as_mut() {
            s.push_str(chunk);
        } else {
            *g = Some(chunk.to_string());
        }
    }
    let _ = app.emit_to(
        "bubble",
        "bubble-chunk",
        BubbleChunkPayload {
            chunk: chunk.to_string(),
        },
    );
    Ok(())
}

/// 流式结束: emit "bubble-end" → 前端启动自动隐藏定时器
pub fn finalize_bubble(app: &AppHandle) -> Result<(), String> {
    let _ = app.emit_to("bubble", "bubble-end", ());
    Ok(())
}

/// 把 bubble 窗口对齐到 pet 窗口正上方
fn position_above_pet(app: &AppHandle, bubble: &tauri::WebviewWindow) {
    let Some(pet) = app.get_webview_window("pet") else {
        return;
    };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else {
        return;
    };
    // bubble 中心对齐 pet 中心，bubble 底部贴近 pet 顶部
    let pet_center_x = pet_pos.x as f64 + pet_size.width as f64 / 2.0;
    let pet_top = pet_pos.y as f64;

    let scale = bubble.scale_factor().unwrap_or(1.0);
    let bubble_w_px = BUBBLE_W * scale;
    let bubble_h_px = BUBBLE_H * scale;

    let bubble_x = (pet_center_x - bubble_w_px / 2.0) as i32;
    let bubble_y = (pet_top - bubble_h_px + 6.0 * scale) as i32; // 三角和精灵略重叠

    let _ = bubble.set_position(PhysicalPosition::new(bubble_x, bubble_y));
}

fn create_bubble_window(app: &AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    WebviewWindowBuilder::new(app, "bubble", WebviewUrl::App("bubble.html".into()))
        .title("8Bit Bubble")
        .inner_size(BUBBLE_W, BUBBLE_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()
}

/// 前端 init 时调用：取出待消费文本（取出后清空），用于解决首次创建时 emit 早于 listen 的时序
#[tauri::command]
pub async fn cmd_consume_bubble_text(
    state: State<'_, SharedBubble>,
) -> Result<Option<String>, String> {
    let mut t = state.pending_text.lock().map_err(|e| e.to_string())?;
    Ok(t.take())
}

/// 前端自动隐藏定时到了 → 调用此 cmd 隐藏窗口
#[tauri::command]
pub async fn cmd_hide_bubble(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("bubble") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_bubble_default_empty() {
        let b = SharedBubble::new();
        let g = b.pending_text.lock().unwrap();
        assert!(g.is_none());
    }

    #[test]
    fn test_shared_bubble_take_clears() {
        let b = SharedBubble::new();
        *b.pending_text.lock().unwrap() = Some("hello".into());
        let taken = b.pending_text.lock().unwrap().take();
        assert_eq!(taken, Some("hello".to_string()));
        // 二次取应为 None
        let again = b.pending_text.lock().unwrap().take();
        assert_eq!(again, None);
    }

    #[test]
    fn test_bubble_constants_reasonable() {
        // 280x140 适合阅读 4-6 行 13px 字体
        assert!(BUBBLE_W >= 240.0 && BUBBLE_W <= 360.0);
        assert!(BUBBLE_H >= 100.0 && BUBBLE_H <= 200.0);
    }

    #[test]
    fn test_payload_serializes() {
        let p = BubblePayload {
            text: "你好".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("你好") || json.contains("\\u"));
    }

    #[test]
    fn test_chunk_payload_serializes() {
        let p = BubbleChunkPayload {
            chunk: "片段".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("chunk"));
        assert!(json.contains("片段") || json.contains("\\u"));
    }

    #[test]
    fn test_pending_accumulates() {
        let b = SharedBubble::new();
        // 模拟 start_streaming 的清空
        *b.pending_text.lock().unwrap() = Some(String::new());
        // 模拟 append 累加
        if let Some(s) = b.pending_text.lock().unwrap().as_mut() {
            s.push_str("Hello");
        }
        if let Some(s) = b.pending_text.lock().unwrap().as_mut() {
            s.push_str(" World");
        }
        let taken = b.pending_text.lock().unwrap().take();
        assert_eq!(taken, Some("Hello World".to_string()));
    }
}
