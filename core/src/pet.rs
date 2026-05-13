//! 桌宠状态机与动画逻辑
//!
//! 纯函数设计：状态转换和帧计算不依赖 ggez，方便单元测试。
//!
//! 动画引擎采用时间轴查表（elapsed-driven）：每帧有独立 duration，
//! 状态分循环态（loop）和瞬变态（repeat + fallback）两类。

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
    /// 游戏进行中
    GamePlay,
    /// 游戏胜利
    GameWin,
    /// 游戏失败
    GameLose,
}

impl PetState {
    /// 时间轴帧定义：`&[(sprite_index, duration_ms)]`
    ///
    /// sprite_index 指向 `SPRITES[state]` 数组的索引（与 JS 侧 SPRITES 映射一致）。
    fn timeline(self) -> &'static [(usize, u64)] {
        match self {
            Self::Idle => &[
                (0, 1500), // 睁眼 — 悠闲
                (1, 120),  // 半眯 — 快速
                (2, 200),  // 闭眼 — 短暂
                (1, 120),  // 半眯 — 恢复
                (0, 1800), // 睁眼 — 深呼吸停顿
            ],
            Self::Walk => &[(0, 150), (1, 150), (0, 150), (2, 150)],
            Self::Sleep => &[(0, 800), (1, 800)],
            Self::Talk => &[(0, 300), (1, 300), (2, 400)],
            Self::Happy => &[(0, 250), (1, 120), (0, 230)],
            Self::Confused => &[(0, 400), (1, 400)],
            Self::GamePlay => &[(0, 300), (1, 300)],
            Self::GameWin => &[(0, 250), (1, 120), (0, 230)],
            Self::GameLose => &[(0, 400), (1, 400)],
        }
    }

    /// 是否为循环态（无限播放）
    #[allow(dead_code)]
    fn loops(self) -> bool {
        matches!(self, Self::Idle | Self::Walk | Self::Sleep | Self::GamePlay)
    }

    /// 瞬态重复次数：播 N 遍后回落到 fallback
    fn repeat_count(self) -> Option<usize> {
        match self {
            Self::Talk => Some(3),
            Self::Happy => Some(3),
            Self::Confused => Some(2),
            Self::GameWin => Some(5),
            Self::GameLose => Some(4),
            _ => None,
        }
    }

    /// 瞬态回落目标状态
    fn fallback(self) -> Option<PetState> {
        match self {
            Self::Talk | Self::Happy | Self::Confused | Self::GameWin | Self::GameLose => {
                Some(PetState::Idle)
            }
            _ => None,
        }
    }

    /// Walk 状态的自动超时（未到达目标时强制停止），其他状态无超时
    pub fn auto_idle_timeout_ms(self) -> Option<u64> {
        if self == Self::Walk { Some(3000) } else { None }
    }

    /// 该状态时间轴中的最大合法 sprite index（用于边界检查）
    #[allow(dead_code)]
    fn max_sprite_index(self) -> usize {
        match self {
            Self::Idle | Self::Walk => 3,
            Self::Sleep => 1,
            Self::Talk => 2,
            Self::Happy => 2,
            Self::Confused => 1,
            Self::GamePlay => 1,
            Self::GameWin => 2,
            Self::GameLose => 1,
        }
    }

    /// 该状态一整遍的时间轴总时长（ms）
    fn pass_duration_ms(self) -> u64 {
        self.timeline().iter().map(|(_, d)| *d).sum()
    }
}

// ---- 宠物实例 ----

#[derive(Debug, Clone)]
pub struct Pet {
    pub state: PetState,
    pub x: f32,
    pub y: f32,
    pub facing_right: bool,
    /// 当前动画帧索引（指向 SPRITES[state] 的数组位置）
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
        Self {
            x,
            y,
            ..Default::default()
        }
    }

    /// 更新状态机，dt 为距上次更新的毫秒数
    pub fn update(&mut self, dt_ms: u64) {
        self.state_time_ms += dt_ms;
        self.frame_time_ms += dt_ms;

        // 时间轴查表推进帧
        self.advance_frame();

        // Walk 状态移动
        if self.state == PetState::Walk
            && let Some(tx) = self.target_x
        {
            let dx = tx - self.x;
            if dx.abs() < self.speed * dt_ms as f32 / 1000.0 {
                self.x = tx;
            } else {
                self.x += dx.signum() * self.speed * dt_ms as f32 / 1000.0;
            }
            self.facing_right = dx > 0.0;
        }

        // Walk 自动超时回退 Idle
        if let Some(timeout) = self.state.auto_idle_timeout_ms()
            && self.state_time_ms >= timeout
        {
            self.set_state(PetState::Idle);
        }
    }

    /// 时间轴查表：根据 frame_time_ms 定位当前应显示的 sprite index
    fn advance_frame(&mut self) {
        let timeline = self.state.timeline();
        let pass = self.state.pass_duration_ms();
        if pass == 0 {
            return;
        }

        // 瞬态：播完 repeat 遍后回落
        if let Some(n) = self.state.repeat_count()
            && self.frame_time_ms >= pass * n as u64
        {
            if let Some(fb) = self.state.fallback() {
                self.set_state(fb);
            }
            return;
        }

        // 定位当前帧：elapsed % pass → 线性扫描
        let mut rem = self.frame_time_ms % pass;
        for &(sprite, dur) in timeline {
            if rem < dur {
                self.frame = sprite;
                return;
            }
            rem -= dur;
        }
        // 兜底：最后一帧
        self.frame = timeline.last().map(|(s, _)| *s).unwrap_or(0);
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

    /// 进入小游戏进行中状态。
    pub fn start_gameplay(&mut self) {
        self.set_state(PetState::GamePlay);
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
    fn test_idle_timeline_non_uniform_durations() {
        // idle: 前 1500ms 保持 sprite 0
        let mut pet = Pet::default();
        pet.update(1499);
        assert_eq!(pet.frame, 0);

        pet.update(1); // 1500ms → sprite 1（半眯）
        assert_eq!(pet.frame, 1);
    }

    #[test]
    fn test_idle_blink_sequence() {
        let mut pet = Pet::default();

        // 1620ms = 1500 + 120 → sprite 2（闭眼）
        pet.update(1619);
        assert_eq!(pet.frame, 1);
        pet.update(1);
        assert_eq!(pet.frame, 2);

        // 1840ms = 1500+120+200+120 → sprite 1（半眯恢复帧）
        // 时间轴：[0,1500)→s0 [1500,1620)→s1 [1620,1820)→s2 [1820,1940)→s1 [1940,∞)→s0
        pet.update(1840 - 1620);
        assert_eq!(pet.frame, 1);

        // 1940ms → sprite 0（深呼吸睁眼）
        pet.update(1940 - 1840);
        assert_eq!(pet.frame, 0);
    }

    #[test]
    fn test_idle_loops_indefinitely() {
        let mut pet = Pet::default();
        let total = PetState::Idle.pass_duration_ms(); // 3740ms

        // 一整圈后应该还在 idle
        pet.update(total);
        assert_eq!(pet.state, PetState::Idle);
        assert_eq!(pet.frame, 0); // 第一帧也是 sprite 0

        // 很长时间后仍然 idle
        pet.update(100_000);
        assert_eq!(pet.state, PetState::Idle);
    }

    #[test]
    fn test_happy_repeat_then_fallback() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Happy);

        let happy_pass = PetState::Happy.pass_duration_ms(); // 600ms
        let total = happy_pass * 3; // 1800ms

        // 还没播完 3 遍
        pet.update(total - 1);
        assert_eq!(pet.state, PetState::Happy);

        // 刚好播完 3 遍 → fallback to idle
        pet.update(1);
        assert_eq!(pet.state, PetState::Idle);
    }

    #[test]
    fn test_confused_repeat_then_fallback() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Confused);

        let confused_pass = PetState::Confused.pass_duration_ms(); // 800ms
        let total = confused_pass * 2; // 1600ms

        pet.update(total - 1);
        assert_eq!(pet.state, PetState::Confused);

        pet.update(1);
        assert_eq!(pet.state, PetState::Idle);
    }

    #[test]
    fn test_talk_repeat_then_fallback() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Talk);

        let talk_pass = PetState::Talk.pass_duration_ms(); // 1000ms
        let total = talk_pass * 3; // 3000ms

        pet.update(total - 1);
        assert_eq!(pet.state, PetState::Talk);

        pet.update(1);
        assert_eq!(pet.state, PetState::Idle);
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
    fn test_walk_auto_idle_timeout() {
        let mut pet = Pet::default();
        pet.set_state(PetState::Walk); // 3000ms 超时

        pet.update(2999);
        assert_eq!(pet.state, PetState::Walk);

        pet.update(1); // 总计 3000ms
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

        pet.start_gameplay();
        assert_eq!(pet.state, PetState::GamePlay);
    }

    #[test]
    fn test_state_timeline_structure() {
        // 验证各状态的 timeline 结构正确
        let idle_tl = PetState::Idle.timeline();
        assert_eq!(idle_tl.len(), 5);
        assert_eq!(idle_tl[0], (0, 1500));
        assert_eq!(idle_tl[4].0, 0); // 最后帧也是 sprite 0（睁眼）

        let walk_tl = PetState::Walk.timeline();
        assert_eq!(walk_tl.len(), 4);

        let sleep_tl = PetState::Sleep.timeline();
        assert_eq!(sleep_tl.len(), 2);

        let talk_tl = PetState::Talk.timeline();
        assert_eq!(talk_tl.len(), 3);

        let happy_tl = PetState::Happy.timeline();
        assert_eq!(happy_tl.len(), 3);

        let confused_tl = PetState::Confused.timeline();
        assert_eq!(confused_tl.len(), 2);

        assert_eq!(PetState::GamePlay.timeline().len(), 2);
        assert_eq!(PetState::GameWin.timeline().len(), 3);
        assert_eq!(PetState::GameLose.timeline().len(), 2);
    }

    #[test]
    fn test_state_loop_and_repeat_classification() {
        // 循环态
        assert!(PetState::Idle.loops());
        assert!(PetState::Walk.loops());
        assert!(PetState::Sleep.loops());
        assert!(PetState::GamePlay.loops());
        assert!(!PetState::Talk.loops());
        assert!(!PetState::Happy.loops());
        assert!(!PetState::Confused.loops());
        assert!(!PetState::GameWin.loops());
        assert!(!PetState::GameLose.loops());

        // 瞬态 repeat count
        assert_eq!(PetState::Talk.repeat_count(), Some(3));
        assert_eq!(PetState::Happy.repeat_count(), Some(3));
        assert_eq!(PetState::Confused.repeat_count(), Some(2));
        assert_eq!(PetState::GameWin.repeat_count(), Some(5));
        assert_eq!(PetState::GameLose.repeat_count(), Some(4));
        assert_eq!(PetState::Idle.repeat_count(), None);
        assert_eq!(PetState::Walk.repeat_count(), None);

        // 瞬态 fallback
        assert_eq!(PetState::Talk.fallback(), Some(PetState::Idle));
        assert_eq!(PetState::Happy.fallback(), Some(PetState::Idle));
        assert_eq!(PetState::Confused.fallback(), Some(PetState::Idle));
        assert_eq!(PetState::GameWin.fallback(), Some(PetState::Idle));
        assert_eq!(PetState::GameLose.fallback(), Some(PetState::Idle));
        assert_eq!(PetState::Idle.fallback(), None);
    }

    #[test]
    fn test_gameplay_does_not_auto_idle() {
        let mut pet = Pet::default();
        pet.set_state(PetState::GamePlay);
        pet.update(100_000);
        assert_eq!(pet.state, PetState::GamePlay);
    }

    #[test]
    fn test_game_result_states_fallback_to_idle() {
        let mut win = Pet::default();
        win.set_state(PetState::GameWin);
        win.update(2999);
        assert_eq!(win.state, PetState::GameWin);
        win.update(1);
        assert_eq!(win.state, PetState::Idle);

        let mut lose = Pet::default();
        lose.set_state(PetState::GameLose);
        lose.update(3199);
        assert_eq!(lose.state, PetState::GameLose);
        lose.update(1);
        assert_eq!(lose.state, PetState::Idle);
    }

    #[test]
    fn test_large_dt_recovery() {
        // 大 dt 后时间轴直接定位正确位置，不 panic
        let mut pet = Pet::default();
        pet.update(50000); // 50 秒
        assert!(pet.frame <= PetState::Idle.max_sprite_index());

        // 瞬态大 dt 应已 fallback
        let mut pet2 = Pet::default();
        pet2.set_state(PetState::Happy);
        pet2.update(99999);
        assert_eq!(pet2.state, PetState::Idle);
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

    // PetState 的 6 个变体
    fn any_state() -> impl Strategy<Value = PetState> {
        prop_oneof![
            Just(PetState::Idle),
            Just(PetState::Walk),
            Just(PetState::Sleep),
            Just(PetState::Talk),
            Just(PetState::Happy),
            Just(PetState::Confused),
            Just(PetState::GamePlay),
            Just(PetState::GameWin),
            Just(PetState::GameLose),
        ]
    }

    // 有 repeat 的瞬态子集
    fn any_transient_state() -> impl Strategy<Value = PetState> {
        prop_oneof![
            Just(PetState::Talk),
            Just(PetState::Happy),
            Just(PetState::Confused),
            Just(PetState::GameWin),
            Just(PetState::GameLose),
        ]
    }

    // === 帧索引合法性 ===

    proptest! {
        #[test]
        fn frame_always_valid_for_state(state in any_state(), dt_ms in 0u64..10_000u64) {
            let mut pet = Pet::default();
            pet.set_state(state);
            pet.update(dt_ms);

            prop_assert!(
                pet.frame <= state.max_sprite_index(),
                "frame {} > max_sprite_index {} for state {:?}",
                pet.frame, state.max_sprite_index(), state
            );
        }
    }

    // === 大 dt 不 panic ===

    proptest! {
        #[test]
        fn large_dt_no_panic(state in any_state()) {
            let mut pet = Pet::default();
            pet.set_state(state);
            pet.update(u64::MAX / 2); // 极大值

            // 只要没 panic 就通过；瞬态可能已 fallback 到 idle
            let max_idx = pet.state.max_sprite_index();
            prop_assert!(pet.frame <= max_idx,
                "frame {} out of range after large dt for {:?}",
                pet.frame, pet.state
            );
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
                    prop_assert!((pet.x - target).abs() < 1.0,
                        "should reach target: x={} target={}", pet.x, target
                    );
                } else {
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
                pet.speed = 1000.0;
                pet.walk_to(target);
                pet.update(100);

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

            pet.set_state(state);
            prop_assert_eq!(pet.frame, frame_after_first,
                "set_state should be idempotent for same state"
            );
        }
    }

    proptest! {
        #[test]
        fn set_state_resets_timers(state in any_state()) {
            let mut pet = Pet::default();
            pet.set_state(PetState::Sleep);
            pet.frame_time_ms = 9999;
            pet.state_time_ms = 8888;
            pet.frame = 3;

            pet.set_state(state);

            // Sleep → Idle 是 no-op（Idle 的 auto_idle_timeout 是 None）
            // Sleep → 其他状态: 重置; 相同状态(非Idle): 不重置
            if state != PetState::Sleep && state != PetState::Idle {
                prop_assert_eq!(pet.frame_time_ms, 0, "frame_time_ms not reset for {:?}", state);
                prop_assert_eq!(pet.state_time_ms, 0, "state_time_ms not reset for {:?}", state);
                prop_assert_eq!(pet.frame, 0, "frame not reset for {:?}", state);
            }
        }
    }

    // === 瞬态 repeat+fallback 属性 ===

    proptest! {
        #[test]
        fn transient_falls_back_after_repeat(
            state in any_transient_state(),
        ) {
            let repeat = state.repeat_count().unwrap();
            let pass = state.pass_duration_ms();
            let total = pass * repeat as u64;
            let mut pet = Pet::default();
            pet.set_state(state);

            // 超时前不切换
            pet.update(total - 1);
            prop_assert_eq!(pet.state, state,
                "should not fallback before {} passes for {:?}", repeat, state
            );

            // 超时后切换到 Idle
            pet.update(1);
            prop_assert_eq!(pet.state, PetState::Idle,
                "should fallback to Idle after {} passes for {:?}", repeat, state
            );
        }
    }

    // === 循环态永不自动切换 ===

    proptest! {
        #[test]
        fn looping_states_stay_put(dt_ms in 1u64..100_000u64) {
            for &state in &[PetState::Idle, PetState::Sleep, PetState::GamePlay] {
                let mut pet = Pet::default();
                pet.set_state(state);
                pet.update(dt_ms);
                prop_assert_eq!(pet.state, state,
                    "{:?} should never auto-fallback after {}ms", state, dt_ms
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
