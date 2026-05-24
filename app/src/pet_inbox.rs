//! 宠物待查看窗口：承接宠物角标点击后的轻量 Inbox。
//!
//! 这个窗口只负责生命周期和定位，具体内容由前端通过现有 IPC 拉取。
//! 它与 pet、Agent Watch、截图观察模块协作，但不拥有这些业务状态。

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const WINDOW_LABEL: &str = "pet-inbox";
const WINDOW_W: f64 = 280.0;
const WINDOW_H: f64 = 260.0;
const EDGE_MARGIN: i32 = 8;
const PET_GAP: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RectI {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectI {
    fn right(self) -> i32 {
        self.x + self.w
    }

    fn bottom(self) -> i32 {
        self.y + self.h
    }

    fn inflate(self, gap: i32) -> Self {
        Self {
            x: self.x - gap,
            y: self.y - gap,
            w: self.w + gap * 2,
            h: self.h + gap * 2,
        }
    }

    fn overlap_area(self, other: Self) -> i32 {
        let x = (self.right().min(other.right()) - self.x.max(other.x)).max(0);
        let y = (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0);
        x * y
    }
}

#[tauri::command]
pub async fn cmd_show_pet_inbox(app: AppHandle) -> Result<(), String> {
    let window = ensure_window(&app).map_err(|e| e.to_string())?;
    position_near_pet(&app, &window);
    window.show().map_err(|e| e.to_string())?;
    window.set_always_on_top(true).map_err(|e| e.to_string())?;
    let _ = window.eval("window.__petInboxRefresh && window.__petInboxRefresh();");
    Ok(())
}

#[tauri::command]
pub async fn cmd_hide_pet_inbox(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn ensure_window(app: &AppHandle) -> Result<WebviewWindow, tauri::Error> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        return Ok(window);
    }
    let window =
        WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("pet_inbox.html".into()))
            .title("BitCat Inbox")
            .inner_size(WINDOW_W, WINDOW_H)
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()?;
    let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
    Ok(window)
}

fn position_near_pet(app: &AppHandle, inbox: &WebviewWindow) {
    let Some(pet) = app
        .get_webview_window("pet")
        .filter(|w| w.is_visible().unwrap_or(false))
        .or_else(|| {
            app.get_webview_window("pet-mini")
                .filter(|w| w.is_visible().unwrap_or(false))
        })
    else {
        return;
    };
    let (Ok(pet_pos), Ok(pet_size)) = (pet.outer_position(), pet.outer_size()) else {
        return;
    };
    let Some(monitor) = pet.current_monitor().ok().flatten() else {
        return;
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let scale = inbox.scale_factor().unwrap_or(1.0).max(0.5);
    let size = inbox.inner_size().unwrap_or(PhysicalSize::new(
        (WINDOW_W * scale).round() as u32,
        (WINDOW_H * scale).round() as u32,
    ));
    let width = size.width as i32;
    let height = size.height as i32;
    let (x, y) = compute_inbox_position(
        RectI {
            x: mon_pos.x,
            y: mon_pos.y,
            w: mon_size.width as i32,
            h: mon_size.height as i32,
        },
        RectI {
            x: pet_pos.x,
            y: pet_pos.y,
            w: pet_size.width as i32,
            h: pet_size.height as i32,
        },
        width,
        height,
    );
    let _ = inbox.set_position(PhysicalPosition::new(x, y));
}

fn compute_inbox_position(monitor: RectI, pet: RectI, width: i32, height: i32) -> (i32, i32) {
    let min_x = monitor.x + EDGE_MARGIN;
    let max_x = monitor.right() - width - EDGE_MARGIN;
    let min_y = monitor.y + EDGE_MARGIN;
    let max_y = monitor.bottom() - height - EDGE_MARGIN;
    let clamp_x = |x: i32| x.clamp(min_x, max_x.max(min_x));
    let clamp_y = |y: i32| y.clamp(min_y, max_y.max(min_y));
    let center_x = pet.x + pet.w / 2 - width / 2;
    let center_y = pet.y + pet.h / 2 - height / 2;
    let candidates = [
        (clamp_x(center_x), clamp_y(pet.bottom() + PET_GAP)),
        (clamp_x(center_x), clamp_y(pet.y - height - PET_GAP)),
        (clamp_x(pet.right() + PET_GAP), clamp_y(center_y)),
        (clamp_x(pet.x - width - PET_GAP), clamp_y(center_y)),
    ];
    let avoid = pet.inflate(PET_GAP);
    let mut best = candidates[0];
    let mut best_score = i32::MAX;
    for (index, &(x, y)) in candidates.iter().enumerate() {
        let area = RectI {
            x,
            y,
            w: width,
            h: height,
        }
        .overlap_area(avoid);
        let score = area.saturating_mul(10) + index as i32;
        if score < best_score {
            best = (x, y);
            best_score = score;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_opens_below_top_pet_without_covering_it() {
        let monitor = RectI {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let pet = RectI {
            x: 250,
            y: 24,
            w: 128,
            h: 128,
        };

        let (x, y) = compute_inbox_position(monitor, pet, 280, 260);

        assert_eq!(x, 174);
        assert!(y >= pet.bottom() + PET_GAP);
        assert_eq!(
            RectI {
                x,
                y,
                w: 280,
                h: 260,
            }
            .overlap_area(pet.inflate(PET_GAP)),
            0
        );
    }

    #[test]
    fn inbox_opens_above_bottom_pet_without_covering_it() {
        let monitor = RectI {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let pet = RectI {
            x: 250,
            y: 440,
            w: 128,
            h: 128,
        };

        let (x, y) = compute_inbox_position(monitor, pet, 280, 260);

        assert_eq!(x, 174);
        assert!(y + 260 <= pet.y - PET_GAP);
        assert_eq!(
            RectI {
                x,
                y,
                w: 280,
                h: 260,
            }
            .overlap_area(pet.inflate(PET_GAP)),
            0
        );
    }
}
