//! Background scheduler for persisted reminders.
//!
//! The scheduler periodically asks core for due reminder records, then renders
//! each fired item through the shared notification window. This keeps reminder
//! execution deterministic and independent from any active agent conversation.
//! It interacts with `notification_window` for UI and `shutdown` for lifecycle.

use tauri::AppHandle;
use tracing::{debug, warn};

const TICK_MS: u64 = 5_000;
const MIN_PERSONALIZER_TIMEOUT_MS: u64 = 500;
const MAX_PERSONALIZER_TIMEOUT_MS: u64 = 10_000;

/// Start the reminder scheduler thread.
pub fn spawn_reminder_scheduler(app: AppHandle) {
    std::thread::Builder::new()
        .name("ai-pad-reminder-scheduler".to_string())
        .spawn(move || loop {
            if crate::shutdown::is_requested() {
                debug!("reminder scheduler shutdown requested");
                break;
            }
            match ai_pad_core::reminder::fire_due_reminders(chrono::Local::now()) {
                Ok(reminders) => {
                    for reminder in reminders {
                        if let Err(e) = show_due_reminder(&app, &reminder) {
                            warn!(error = %e, reminder_id = %reminder.id, "show reminder notification failed");
                        }
                    }
                }
                Err(e) => warn!(error = %e, "reminder scheduler tick failed"),
            }
            std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
        })
        .expect("failed to spawn reminder scheduler");
}

fn show_due_reminder(
    app: &AppHandle,
    reminder: &ai_pad_core::reminder::ReminderRecord,
) -> Result<(), String> {
    let copy = personalize_due_reminder(reminder);
    crate::notification_window::show_reminder_notification_with_copy(app, reminder, copy)
}

fn personalize_due_reminder(
    reminder: &ai_pad_core::reminder::ReminderRecord,
) -> Option<ai_pad_core::reminder_personalizer::ReminderNotificationCopy> {
    let settings = ai_pad_core::app_settings::AppSettings::load();
    if !settings.appearance.reminder_ai_personalization_enabled {
        return None;
    }

    let timeout_ms = settings
        .appearance
        .reminder_ai_timeout_ms
        .clamp(MIN_PERSONALIZER_TIMEOUT_MS, MAX_PERSONALIZER_TIMEOUT_MS);
    let ai_config = match ai_pad_core::ai_config::AiConfig::load() {
        Ok(config) => config,
        Err(e) => {
            warn!(error = %e, reminder_id = %reminder.id, "reminder personalizer config unavailable");
            return None;
        }
    };
    let reminder = reminder.clone();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            warn!(error = %e, reminder_id = %reminder.id, "reminder personalizer runtime failed");
            return None;
        }
    };
    match runtime.block_on(tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        ai_pad_core::reminder_personalizer::personalize_reminder_notification(
            &ai_config, &reminder,
        ),
    )) {
        Ok(Ok(copy)) => Some(copy),
        Ok(Err(e)) => {
            warn!(error = %e, reminder_id = %reminder.id, "reminder personalization failed");
            None
        }
        Err(_) => {
            warn!(timeout_ms, reminder_id = %reminder.id, "reminder personalization timed out");
            None
        }
    }
}
