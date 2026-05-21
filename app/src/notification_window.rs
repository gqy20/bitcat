//! Lightweight island-style notification window.
//!
//! This module provides a shared transient notification surface for reminders
//! and future background events. It owns only window creation, placement, and
//! small IPC actions; event producers pass structured payloads in.
//! The frontend renders the payload and decides when to fade out.

use ai_pad_core::{
    app_settings::AppSettings, reminder::ReminderRecord,
    reminder_personalizer::ReminderNotificationCopy,
};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{debug, warn};
use windows::{
    core::PCWSTR,
    Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT},
};

const WINDOW_LABEL: &str = "notification";
const WINDOW_W: f64 = 240.0;
const WINDOW_H: f64 = 75.0;
const WINDOW_MAX_H: f64 = 152.0;

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
    show_reminder_notification_with_copy(app, reminder, None)
}

/// Show a reminder with optional AI-personalized notification copy.
pub fn show_reminder_notification_with_copy(
    app: &AppHandle,
    reminder: &ReminderRecord,
    copy: Option<ReminderNotificationCopy>,
) -> Result<(), String> {
    let (title, body, tone) = match copy {
        Some(copy) => (
            copy.title,
            if copy.body.is_empty() {
                None
            } else {
                Some(copy.body)
            },
            copy.tone.as_str().to_string(),
        ),
        None => (
            reminder.title.clone(),
            reminder.message.clone(),
            "warning".to_string(),
        ),
    };
    let payload = NotificationPayload {
        id: format!("reminder-{}-{}", reminder.id, reminder.fire_count),
        title,
        body,
        tone,
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
    play_notification_sound(&payload);
    debug!(id = %payload.id, source = %payload.source, "notification shown");
    Ok(())
}

fn play_notification_sound(payload: &NotificationPayload) {
    let settings = AppSettings::load();
    if !should_play_notification_sound(payload, &settings) {
        return;
    }

    let sound: Vec<u16> = system_sound_alias(&payload.tone)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        PlaySoundW(
            PCWSTR(sound.as_ptr()),
            None,
            SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
        )
    };
    if !ok.as_bool() {
        warn!(tone = %payload.tone, source = %payload.source, "notification sound failed");
    }
}

fn should_play_notification_sound(payload: &NotificationPayload, settings: &AppSettings) -> bool {
    let appearance = &settings.appearance;
    if !appearance.notification_sound_enabled {
        return false;
    }
    match payload.source.as_str() {
        "reminder" => appearance.notification_sound_reminder,
        "agent_watch" => {
            appearance.notification_sound_agent_watch
                && !(appearance.notification_sound_skip_agent_tts && settings.agent_watch.use_tts)
        }
        _ => true,
    }
}

fn system_sound_alias(tone: &str) -> &'static str {
    match tone {
        "success" => "SystemAsterisk",
        "warning" => "SystemExclamation",
        "danger" => "SystemHand",
        _ => "SystemDefault",
    }
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

/// Resize the notification window to fit expanded reminder content.
#[tauri::command]
pub async fn cmd_notification_resize(app: AppHandle, height: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
        let h = height.clamp(WINDOW_H, WINDOW_MAX_H);
        window
            .set_size(PhysicalSize::new(
                (WINDOW_W * scale).round() as u32,
                (h * scale).round() as u32,
            ))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(source: &str, tone: &str) -> NotificationPayload {
        NotificationPayload {
            id: "n-1".to_string(),
            title: "Notice".to_string(),
            body: None,
            tone: tone.to_string(),
            source: source.to_string(),
            reminder_id: None,
            ttl_ms: 12_000,
            actions: Vec::new(),
        }
    }

    #[test]
    fn notification_sound_respects_source_settings() {
        let mut settings = AppSettings::default();
        assert!(should_play_notification_sound(
            &payload("reminder", "warning"),
            &settings
        ));

        settings.appearance.notification_sound_reminder = false;
        assert!(!should_play_notification_sound(
            &payload("reminder", "warning"),
            &settings
        ));

        settings.appearance.notification_sound_reminder = true;
        settings.appearance.notification_sound_enabled = false;
        assert!(!should_play_notification_sound(
            &payload("reminder", "warning"),
            &settings
        ));
    }

    #[test]
    fn agent_watch_sound_can_skip_when_tts_is_enabled() {
        let mut settings = AppSettings::default();
        settings.agent_watch.use_tts = true;
        assert!(!should_play_notification_sound(
            &payload("agent_watch", "success"),
            &settings
        ));

        settings.appearance.notification_sound_skip_agent_tts = false;
        assert!(should_play_notification_sound(
            &payload("agent_watch", "success"),
            &settings
        ));
    }

    #[test]
    fn maps_notification_tone_to_system_sound() {
        assert_eq!(system_sound_alias("success"), "SystemAsterisk");
        assert_eq!(system_sound_alias("warning"), "SystemExclamation");
        assert_eq!(system_sound_alias("danger"), "SystemHand");
        assert_eq!(system_sound_alias("info"), "SystemDefault");
    }
}
