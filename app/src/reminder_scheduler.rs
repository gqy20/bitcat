//! Background scheduler for persisted reminders.
//!
//! The scheduler periodically asks core for due reminder records, then renders
//! each fired item through the shared notification window. This keeps reminder
//! execution deterministic and independent from any active agent conversation.
//! It interacts with `notification_window` for UI and `shutdown` for lifecycle.

use tauri::AppHandle;
use tracing::{debug, warn};

const TICK_MS: u64 = 5_000;

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
                        if let Err(e) =
                            crate::notification_window::show_reminder_notification(&app, &reminder)
                        {
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
