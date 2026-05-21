//! Lightweight island-style notification window.
//!
//! This module provides a shared transient notification surface for reminders
//! and future background events. It owns only window creation, placement, and
//! small IPC actions; event producers pass structured payloads in.
//! The frontend renders the payload and decides when to fade out.

use ai_pad_core::reminder::ReminderRecord;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{debug, warn};

const WINDOW_LABEL: &str = "notification";
const WINDOW_W: f64 = 240.0;
const WINDOW_H: f64 = 75.0;

/// Button shown on a transient notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

/// Payload sent to the notification frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub tone: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminder_id: Option<String>,
    pub ttl_ms: u64,
    #[serde(default)]
    pub actions: Vec<NotificationAction>,
}

/// Precreate the notification window so the first reminder can fade in quickly.
pub fn precreate_notification_window(app: &AppHandle) -> Result<(), tauri::Error> {
    if app.get_webview_window(WINDOW_LABEL).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL,
        WebviewUrl::App("notification.html".into()),
    )
    .title("8Bit Notification")
    .inner_size(WINDOW_W, WINDOW_H)
    .decorations(false)
    .transparent(true)
    .background_color(tauri::webview::Color(0, 0, 0, 0))
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .visible(false)
    .build()?;
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    Ok(())
}

/// Show a reminder as a notification.
pub fn show_reminder_notification(
    app: &AppHandle,
    reminder: &ReminderRecord,
) -> Result<(), String> {
    let payload = NotificationPayload {
        id: format!("reminder-{}-{}", reminder.id, reminder.fire_count),
        title: reminder.title.clone(),
        body: reminder.message.clone(),
        tone: "warning".to_string(),
        source: "reminder".to_string(),
        reminder_id: Some(reminder.id.clone()),
        ttl_ms: 12_000,
        actions: vec![
            NotificationAction {
                id: "complete".to_string(),
                label: "完成".to_string(),
            },
            NotificationAction {
                id: "snooze_10".to_string(),
                label: "10 分钟后".to_string(),
            },
        ],
    };
    show_notification(app, payload)
}

/// Show a structured notification payload.
pub fn show_notification(app: &AppHandle, payload: NotificationPayload) -> Result<(), String> {
    let window = ensure_notification_window(app).map_err(|e| e.to_string())?;
    position_top_center(app, &window);
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let _ = window.set_size(PhysicalSize::new(
        (WINDOW_W * scale).round() as u32,
        (WINDOW_H * scale).round() as u32,
    ));
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    let _ = window.show();
    let _ = window.set_always_on_top(true);

    let json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let _ = window.emit("notification-show", &payload);
    let _ = window.eval(format!(
        "if(window.__notificationShow)window.__notificationShow({json});"
    ));
    debug!(id = %payload.id, source = %payload.source, "notification shown");
    Ok(())
}

fn ensure_notification_window(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Ok(window);
    }
    precreate_notification_window(app)?;
    Ok(app
        .get_webview_window(WINDOW_LABEL)
        .expect("notification window should exist after precreate"))
}

fn position_top_center(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .get_webview_window("pet")
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let pos = monitor.position();
    let size = monitor.size();
    let width = (WINDOW_W * scale).round() as i32;
    let x = pos.x + ((size.width as i32 - width) / 2).max(12);
    let y = pos.y + (7.0 * scale).round() as i32;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Hide the notification window.
#[tauri::command]
pub async fn cmd_notification_hide(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Handle a frontend action for reminder notifications.
#[tauri::command]
pub async fn cmd_notification_action(
    app: AppHandle,
    action: String,
    reminder_id: Option<String>,
) -> Result<(), String> {
    let mut reminder_changed = false;
    if let Some(id) = reminder_id {
        match action.as_str() {
            "complete" => {
                ai_pad_core::reminder::complete_reminder_with_source(&id, "notification")?;
                reminder_changed = true;
            }
            "snooze_10" => {
                ai_pad_core::reminder::snooze_reminder_with_source(&id, 10, "notification")?;
                reminder_changed = true;
            }
            "cancel" => {
                ai_pad_core::reminder::cancel_reminder_with_source(&id, "notification")?;
                reminder_changed = true;
            }
            "dismiss" => {}
            other => warn!(action = other, reminder_id = %id, "unknown notification action"),
        }
    }
    if reminder_changed {
        let _ = app.emit("reminders-updated", ());
    }
    cmd_notification_hide(app).await
}
