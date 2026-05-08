/// 桌宠窗口 — 基于 ggez 0.10 的渲染与事件循环

use ai_pad_core::pet::{Pet, PetState};
use ggez::{
    conf::{WindowMode, WindowSetup},
    event,
    graphics::{self, Canvas, Color, DrawParam, Mesh, Rect},
    Context, GameResult,
};

const SPRITE_W: usize = 16;
const SPRITE_H: usize = 16;

const IDLE_0: &[u8] = &[
    0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
    0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
    0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
    0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
    0,1,2,2,3,2,2,2,2,2,3,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,4,4,2,2,2,2,2,4,4,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
    0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

const SLEEP_0: &[u8] = &[
    0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
    0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
    0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
    0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
    0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

const TALK_0: &[u8] = &[
    0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
    0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
    0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
    0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
    0,1,2,2,3,2,2,2,2,2,3,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,4,4,2,2,2,2,2,4,4,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,5,2,0,2,5,2,2,2,1,0,0,
    0,1,2,2,2,2,0,0,0,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

const HAPPY_0: &[u8] = &[
    0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
    0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
    0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
    0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
    0,1,2,2,3,2,2,2,2,2,3,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,0,0,2,2,2,2,2,0,0,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
    0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

const CONFUSED_0: &[u8] = &[
    0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
    0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
    0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
    0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
    0,1,2,2,4,2,2,2,2,2,4,2,2,1,0,0,
    0,1,2,2,2,4,2,2,2,2,4,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
    0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

fn pixel_color(value: u8) -> Option<Color> {
    match value {
        0 => None,
        1 => Some(Color::from((30, 30, 40, 255))),
        2 => Some(Color::from((255, 180, 140, 255))),
        3 => Some(Color::from((255, 220, 190, 255))),
        4 => Some(Color::from((40, 35, 50, 255))),
        5 => Some(Color::from((255, 120, 140, 255))),
        _ => None,
    }
}

fn sprite_for_state(state: PetState, _frame: usize) -> &'static [u8] {
    match state {
        PetState::Idle | PetState::Walk => IDLE_0,
        PetState::Sleep => SLEEP_0,
        PetState::Talk => TALK_0,
        PetState::Happy => HAPPY_0,
        PetState::Confused => CONFUSED_0,
    }
}

pub struct PetWindow {
    pub pet: Pet,
    window_size: (f32, f32),
}

impl PetWindow {
    pub fn new() -> Self {
        Self { pet: Pet::new(64.0, 64.0), window_size: (128.0, 128.0) }
    }
    pub fn run() -> GameResult {
        let (ctx, event_loop) = ggez::ContextBuilder::new("ai-pad-pet", "ai-pad")
            .window_setup(WindowSetup::default().title("8Bit Cat"))
            .window_mode(WindowMode::default().dimensions(128.0, 128.0).resizable(false))
            .build()?;
        event::run(ctx, event_loop, Self::new())
    }
}

impl event::EventHandler for PetWindow {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        let dt_ms = (ctx.time.delta().as_secs_f32() * 1000.0) as u64;
        self.pet.update(dt_ms);
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        let mut canvas = Canvas::from_frame(ctx, Color::from((0, 0, 0, 0)));
        let scale = self.window_size.0 / SPRITE_W as f32;
        let ox = (self.window_size.0 - SPRITE_W as f32 * scale) / 2.0;
        let oy = (self.window_size.1 - SPRITE_H as f32 * scale) / 2.0;
        let bounce = if self.pet.state == PetState::Walk {
            (self.pet.frame_time_ms as f32 / 75.0).sin() * 2.0
        } else { 0.0 };
        draw_sprite_pixels(&mut canvas, ctx, self.pet.state, self.pet.frame, ox, oy + bounce, scale);
        canvas.finish(ctx)
    }
}

/// 内联像素绘制（避免每帧创建大量 Mesh 对象）
fn draw_sprite_pixels(canvas: &mut Canvas, ctx: &mut Context, state: PetState, frame: usize, x: f32, y: f32, scale: f32) {
    let data = sprite_for_state(state, frame);
    for row in 0..SPRITE_H {
        for col in 0..SPRITE_W {
            let idx = row * SPRITE_W + col;
            if let Some(color) = pixel_color(data[idx]) {
                let rect = Rect::new(x + col as f32 * scale, y + row as f32 * scale, scale, scale);
                if let Ok(mesh) = Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), rect, color) {
                    canvas.draw(&mesh, DrawParam::default());
                }
            }
        }
    }
}
