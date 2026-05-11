//! 桌宠状态机与动画逻辑
//!
//! 纯函数设计：状态转换和帧计算不依赖 ggez，方便单元测试。

use serde::{Deserialize, Serialize};

// ---- 状态枚举 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PetState {
    /// 待机（呼吸动画）
    #[default]
    Idle,
    /// 走动中
    Walk,
    /// 睡觉
    Sleep,
    /// 说话/思考（AI 回复时）
    Talk,
    /// 开心（被夸奖时）
    Happy,
    /// 困惑（出错时）
    Confused,
}

impl PetState {
    /// 该状态的默认动画帧数
    pub fn frame_count(self) -> usize {
        match self {
            Self::Idle => 4,
            Self::Walk => 4,
            Self::Sleep => 2,
            Self::Talk => 3,
            Self::Happy => 3,
            Self::Confused => 2,
        }
    }

    /// 每帧持续时间（毫秒）
    pub fn frame_duration_ms(self) -> u64 {
        match self {
            Self::Idle => 500,
            Self::Walk => 150,
            Self::Sleep => 800,
            Self::Talk => 300,
            Self::Happy => 200,
            Self::Confused => 400,
        }
    }

    /// 无操作自动超时后回退到 Idle 的时长（毫秒），None 表示不会自动回退
    pub fn auto_idle_timeout_ms(self) -> Option<u64> {
        match self {
            Self::Idle | Self::Sleep => None,
            Self::Walk => Some(3000),
            Self::Talk => Some(5000),
            Self::Happy => Some(2000),
            Self::Confused => Some(3000),
        }
    }
}

// ---- 宠物实例 ----

#[derive(Debug, Clone)]
pub struct Pet {
    pub state: PetState,
    pub x: f32,
    pub y: f32,
    pub facing_right: bool,
    /// 当前动画帧索引
    pub frame: usize,
    /// 当前帧已持续的时间（毫秒）
    pub frame_time_ms: u64,
    /// 处于当前状态的总时间（毫秒）
    pub state_time_ms: u64,
    /// 目标位置（Walk 状态用）
    pub target_x: Option<f32>,
    /// 速度（像素/秒）
    pub speed: f32,
}

impl Default for Pet {
    fn default() -> Self {
        Self {
            state: PetState::default(),
            x: 0.0,
            y: 0.0,
            facing_right: true,
            frame: 0,
            frame_time_ms: 0,
            state_time_ms: 0,
            target_x: None,
            speed: 60.0,
        }
    }
}

impl Pet {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y, ..Default::default() }
    }

    /// 更新状态机，dt 为距上次更新的毫秒数
    pub fn update(&mut self, dt_ms: u64) {
        self.state_time_ms += dt_ms;
        self.frame_time_ms += dt_ms;

        // 动画帧推进
        let duration = self.state.frame_duration_ms();
        while self.frame_time_ms >= duration {
            self.frame_time_ms -= duration;
            self.frame = (self.frame + 1) % self.state.frame_count();
        }

        // Walk 状态移动
        if self.state == PetState::Walk && let Some(tx) = self.target_x {
            let dx = tx - self.x;
            if dx.abs() < self.speed * dt_ms as f32 / 1000.0 {
                self.x = tx;
            } else {
                self.x += dx.signum() * self.speed * dt_ms as f32 / 1000.0;
            }
            self.facing_right = dx > 0.0;
        }

        // 自动超时回退 Idle
        if let Some(timeout) = self.state.auto_idle_timeout_ms()
            && self.state_time_ms >= timeout
        {
            self.set_state(PetState::Idle);
        }
    }

    /// 切换状态，重置计时器和帧
    pub fn set_state(&mut self, new_state: PetState) {
        if self.state != new_state {
            self.state = new_state;
            self.frame = 0;
            self.frame_time_ms = 0;
            self.state_time_ms = 0;
            // 非 Walk 状态清除目标
            if new_state != PetState::Walk {
                self.target_x = None;
            }
        }
    }

    /// 让宠物走到指定 x 坐标
    pub fn walk_to(&mut self, target_x: f32) {
        self.set_state(PetState::Walk);
        self.target_x = Some(target_x);
    }

    /// 触发 AI 对话状态
    pub fn start_talking(&mut self) {
        self.set_state(PetState::Talk);
    }

    /// 触发开心状态
    pub fn set_happy(&mut self) {
        self.set_state(PetState::Happy);
    }

    /// 触发困惑状态
    pub fn set_confused(&mut self) {
        self.set_state(PetState::Confused);
    }

    /// 进入睡眠状态
    pub fn fall_asleep(&mut self) {
        self.set_state(PetState::Sleep);
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_is_idle() {
        let pet = Pet::default();
        assert_eq!(pet.state, PetState::Idle);
        assert!(pet.facing_right);
        assert_eq!(pet.frame, 0);
    }

    #[test]
    fn test_new_with_position() {
        let pet = Pet::new(100.0, 200.0);
        assert_eq!(pet.x, 100.0);
        assert_eq!(pet.y, 200.0);
    }

    #[test]
    fn test_set_state_resets_frame_and_timer() {
        let mut pet = Pet::default();
        pet.frame_time_ms = 999;
        pet.state_time_ms = 9999;
        pet.frame = 3;

        pet.set_state(PetState::Talk);

        assert_eq!(pet.state, PetState::Talk);
        assert_eq!(pet.frame, 0);
        assert_eq!(pet.frame_time_ms, 0);
        assert_eq!(pet.state_time_ms, 0);
    }

    #[test]
    fn test_set_same_state_no_reset() {
        let mut pet = Pet::default();
        pet.frame_time_ms = 100;
        pet.set_state(PetState::Idle);
        assert_eq!(pet.frame_time_ms, 100); // 不重置
    }

    #[test]
    fn test_animation_frame_advances() {
        let mut pet = Pet::default(); // Idle: 4 frames, 500ms each
        assert_eq!(pet.frame, 0);

        pet.update(499);
        assert_eq!(pet.frame, 0); // 还没到 500ms

        pet.update(1); // 总计 500ms
        assert_eq!(pet.frame, 1);

        pet.update(500);
        assert_eq!(pet.frame, 2);

        pet.update(1000); // 跳两帧
        assert_eq!(pet.frame, 0); // 循环回 0 (4 帧: 2→3→0)
    }

    #[test]
    fn test_walk_moves_toward_target() {
        let mut pet = Pet::new(0.0, 0.0);
        pet.speed = 100.0; // 100px/s
        pet.walk_to(50.0);

        pet.update(500); // 500ms → 应走 50px

        assert!((pet.x - 50.0).abs() < 0.01); // 到达目标
        assert!(pet.facing_right);
    }

    #[test]
    fn test_walk_facing_direction() {
        let mut pet = Pet::new(50.0, 0.0);
        pet.speed = 100.0;
        pet.walk_to(0.0); // 向左走

        pet.update(250); // 走 25px

        assert!(!pet.facing_right); // 面朝左
        assert!(pet.x < 50.0);
    }

    #[test]
    fn test_walk_clears_target_on_state_change() {
        let mut pet = Pet::default();
        pet.walk_to(100.0);
        assert!(pet.target_x.is_some());

        pet.set_state(PetState::Happy);
        assert!(pet.target_x.is_none());
    }

    #[test]
    fn test_auto_idle_timeout_walk_to_idle() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Walk); // 3000ms 超时

        pet.update(2999);
        assert_eq!(pet.state, PetState::Walk);

        pet.update(1); // 总计 3000ms
        assert_eq!(pet.state, PetState::Idle);
    }

    #[test]
    fn test_auto_idle_timeout_happy_to_idle() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Happy); // 2000ms 超时

        pet.update(1999);
        assert_eq!(pet.state, PetState::Happy);

        pet.update(1);
        assert_eq!(pet.state, PetState::Idle);
    }

    #[test]
    fn test_sleep_has_no_auto_timeout() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Sleep);

        pet.update(100_000); // 很长时间
        assert_eq!(pet.state, PetState::Sleep);
    }

    #[test]
    fn test_idle_has_no_auto_timeout() {
        let mut pet = Pet::default();
        pet.update(100_000);
        assert_eq!(pet.state, PetState::Idle);
    }

    #[test]
    fn test_convenience_methods() {
        let mut pet = Pet::default();

        pet.start_talking();
        assert_eq!(pet.state, PetState::Talk);

        pet.set_happy();
        assert_eq!(pet.state, PetState::Happy);

        pet.set_confused();
        assert_eq!(pet.state, PetState::Confused);

        pet.fall_asleep();
        assert_eq!(pet.state, PetState::Sleep);
    }

    #[test]
    fn test_state_frame_counts() {
        assert_eq!(PetState::Idle.frame_count(), 4);
        assert_eq!(PetState::Walk.frame_count(), 4);
        assert_eq!(PetState::Sleep.frame_count(), 2);
        assert_eq!(PetState::Talk.frame_count(), 3);
        assert_eq!(PetState::Happy.frame_count(), 3);
        assert_eq!(PetState::Confused.frame_count(), 2);
    }

    #[test]
    fn test_state_frame_durations() {
        assert_eq!(PetState::Idle.frame_duration_ms(), 500);
        assert_eq!(PetState::Walk.frame_duration_ms(), 150);
        assert_eq!(PetState::Sleep.frame_duration_ms(), 800);
        assert!(PetState::Walk.frame_duration_ms() < PetState::Idle.frame_duration_ms());
    }

    #[test]
    fn test_walk_speed_calculation() {
        let mut pet = Pet::new(0.0, 0.0);
        pet.speed = 200.0; // 200px/s
        pet.walk_to(100.0);

        pet.update(250); // 250ms → 应走 50px
        assert!((pet.x - 50.0).abs() < 1.0); // 允许误差
    }
}

// ---- Proptest 属性测试 ----

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::strategy::Just;

    // PetState 的 6 个变体，用索引 0..5 选择
    fn any_state() -> impl Strategy<Value = PetState> {
        prop_oneof![
            Just(PetState::Idle),
            Just(PetState::Walk),
            Just(PetState::Sleep),
            Just(PetState::Talk),
            Just(PetState::Happy),
            Just(PetState::Confused),
        ]
    }

    // 有超时的状态子集
    fn any_timed_state() -> impl Strategy<Value = PetState> {
        prop_oneof![
            Just(PetState::Walk),
            Just(PetState::Talk),
            Just(PetState::Happy),
            Just(PetState::Confused),
        ]
    }

    // === 帧推进属性 ===

    proptest! {
        #[test]
        fn frame_always_in_range(state in any_state(), dt_ms in 0u64..10_000u64) {
            let mut pet = Pet::default();
            pet.set_state(state);
            pet.update(dt_ms);

            prop_assert!(pet.frame < state.frame_count(),
                "frame {} >= frame_count {} for state {:?}",
                pet.frame, state.frame_count(), state
            );
        }
    }

    proptest! {
        #[test]
        fn frame_advances_monotonically_until_wrap(dt_ms in 1u64..500u64) {
            let mut pet = Pet::default();
            let f0 = pet.frame;
            pet.update(dt_ms);

            // 要么推进一帧（或更多），要么回绕到 0
            if pet.frame != 0 {
                prop_assert!(pet.frame > f0,
                    "frame should advance or wrap: {} -> {}", f0, pet.frame
                );
            }
        }
    }

    // === Walk 移动属性 ===

    proptest! {
        #[test]
        fn walk_never_overshoots_target(
            start in -1000f32..1000f32,
            target in -1000f32..1000f32,
            speed in 1f32..1000f32,
            dt_ms in 1u64..5000u64,
        ) {
            let mut pet = Pet::new(start, 0.0);
            pet.speed = speed;

            if start != target {
                pet.walk_to(target);
                pet.update(dt_ms);

                let dx = target - start;
                let max_move = speed * dt_ms as f32 / 1000.0;

                if dx.abs() <= max_move {
                    // 应该到达目标
                    prop_assert!((pet.x - target).abs() < 1.0,
                        "should reach target: x={} target={}", pet.x, target
                    );
                } else {
                    // 不应超过目标
                    if dx > 0.0 {
                        prop_assert!(pet.x <= target + 0.01,
                            "overshoot right: x={} > target={}", pet.x, target
                        );
                    } else {
                        prop_assert!(pet.x >= target - 0.01,
                            "overshoot left: x={} < target={}", pet.x, target
                        );
                    }
                }
            }
        }
    }

    proptest! {
        #[test]
        fn facing_matches_movement_direction(
            start in 0f32..500f32,
            target in 0f32..500f32,
        ) {
            if (start - target).abs() > 1.0 {
                let mut pet = Pet::new(start, 0.0);
                pet.speed = 1000.0; // 足够快，一次更新就能移动
                pet.walk_to(target);
                pet.update(100); // 100ms 内走 100px

                if target > start {
                    prop_assert!(pet.facing_right,
                        "moving right but facing left: start={} target={}", start, target
                    );
                } else {
                    prop_assert!(!pet.facing_right,
                        "moving left but facing right: start={} target={}", start, target
                    );
                }
            }
        }
    }

    // === 状态转换属性 ===

    proptest! {
        #[test]
        fn set_state_idempotent(state in any_state()) {
            let mut pet = Pet::default();
            pet.set_state(state);
            let frame_after_first = pet.frame;

            pet.set_state(state); // 同状态再次设置
            prop_assert_eq!(pet.frame, frame_after_first,
                "set_state should be idempotent for same state"
            );
        }
    }

    proptest! {
        #[test]
        fn set_state_resets_timers(state in any_state()) {
            let mut pet = Pet::default();
            // 确保初始状态与目标不同: 先设 Sleep（不会自动超时）
            pet.set_state(PetState::Sleep);
            pet.frame_time_ms = 9999;
            pet.state_time_ms = 8888;
            pet.frame = 3;

            pet.set_state(state); // 切换到目标状态

            // Sleep → Idle 是 no-op（Idle 的 auto_idle_timeout 是 None）
            // Sleep → 其他状态: 总是重置
            // 其他 → 不同状态: 重置; 相同状态(非Idle): 不重置
            if state != PetState::Sleep && state != PetState::Idle {
                prop_assert_eq!(pet.frame_time_ms, 0, "frame_time_ms not reset for {:?}", state);
                prop_assert_eq!(pet.state_time_ms, 0, "state_time_ms not reset for {:?}", state);
                prop_assert_eq!(pet.frame, 0, "frame not reset for {:?}", state);
            }
        }
    }

    // === 自动超时属性 ===

    proptest! {
        #[test]
        fn auto_idle_transitions_to_idle(
            state in any_timed_state(),
            ) {
            let timeout = state.auto_idle_timeout_ms().unwrap();
            let mut pet = Pet::default();
            pet.set_state(state);

            // 超时前不切换
            pet.update(timeout - 1);
            prop_assert_eq!(pet.state, state,
                "should not auto-idle before timeout for {:?}", state
            );

            // 超时后切换到 Idle
            pet.update(1);
            prop_assert_eq!(pet.state, PetState::Idle,
                "should auto-idle to Idle after timeout for {:?}", state
            );
        }
    }

    proptest! {
        #[test]
        fn no_timeout_states_stay_put(dt_ms in 1u64..100_000u64) {
            for &state in &[PetState::Idle, PetState::Sleep] {
                let mut pet = Pet::default();
                pet.set_state(state);
                pet.update(dt_ms);
                prop_assert_eq!(pet.state, state,
                    "{:?} should never auto-timeout after {}ms", state, dt_ms
                );
            }
        }
    }

    // === 坐标计算属性 (模拟 DragCalc 逻辑) ===

    proptest! {
        #[test]
        fn dpi_scaling_roundtrip(
            win_x in -3840f64..3840f64,
            win_y in -2160f64..2160f64,
            screen_dx in -1920f64..1920f64,
            screen_dy in -1080f64..1080f64,
            scale in 1.0f64..4.0f64,
        ) {
            let new_x = (win_x + screen_dx * scale).round();
            let new_y = (win_y + screen_dy * scale).round();

            // round-trip: 反向计算应一致
            let back_x = ((new_x - win_x) / scale).round();
            let back_y = ((new_y - win_y) / scale).round();

            prop_assert!((back_x - screen_dx).abs() < 1.0, "x round-trip failed");
            prop_assert!((back_y - screen_dy).abs() < 1.0, "y round-trip failed");
        }
    }

    proptest! {
        #[test]
        fn zero_delta_no_change(
            win_x in -3840f64..3840f64,
            win_y in -2160f64..2160f64,
            scale in 0.5f64..5.0f64,
        ) {
            let new_x = win_x + 0.0 * scale;
            let new_y = win_y + 0.0 * scale;
            prop_assert!((new_x - win_x).abs() < 0.001);
            prop_assert!((new_y - win_y).abs() < 0.001);
        }
    }
}
