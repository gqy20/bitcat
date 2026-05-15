//! 表现会话状态与跨功能占用管理。
//!
//! 表现会话把固定舞蹈、音乐响应舞动和小游戏庆祝这类“会临时接管宠物表现层”的流程统一建模。
//! core 只保存可查询的轻量状态，app 层负责把会话事件发给前端并处理窗口、渲染和音频等平台细节。
//! bubble、screenshot 等旁路系统只需要问 `is_performing()`，避免继续耦合到某一种舞蹈实现。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// 表现会话的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerformanceKind {
    /// 基于时间轴和 DanceDef 的固定编排舞蹈。
    ChoreographedDance,
    /// 基于电脑音乐分析帧的响应式舞动。
    MusicReactiveDance,
    /// 游戏或小游戏接管宠物表现层。
    Game,
}

/// 表现会话当前阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerformancePhase {
    /// 已创建会话，前后端正在准备资源。
    Starting,
    /// 正在接管表现层。
    Active,
    /// 会话仍在，但输入静默，例如音乐无声。
    IdleSilence,
    /// 正在收尾归位。
    Stopping,
    /// 会话失败，错误信息记录在 `PerformanceSession::error`。
    Failed,
}

impl PerformancePhase {
    fn blocks_background_work(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Active | Self::IdleSilence | Self::Stopping
        )
    }
}

impl PerformanceKind {
    fn blocks_screenshot_observation(self) -> bool {
        matches!(self, Self::ChoreographedDance | Self::Game)
    }
}

/// 当前表现会话快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceSession {
    /// 单调递增的会话 ID，用来丢弃过期的 stop/frame。
    pub id: u64,
    /// 会话类型。
    pub kind: PerformanceKind,
    /// 当前阶段。
    pub phase: PerformancePhase,
    /// 创建时间，Unix 毫秒。
    pub started_at_ms: u64,
    /// 最近一次状态更新时间，Unix 毫秒。
    pub updated_at_ms: u64,
    /// 最近错误，只有 Failed 阶段通常会填。
    pub error: Option<String>,
}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static CURRENT_SESSION: OnceLock<Mutex<Option<PerformanceSession>>> = OnceLock::new();

fn current_slot() -> &'static Mutex<Option<PerformanceSession>> {
    CURRENT_SESSION.get_or_init(|| Mutex::new(None))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 开始一个新的表现会话，并替换任何旧会话。
pub fn start_performance(kind: PerformanceKind) -> PerformanceSession {
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    let now = now_ms();
    let session = PerformanceSession {
        id,
        kind,
        phase: PerformancePhase::Starting,
        started_at_ms: now,
        updated_at_ms: now,
        error: None,
    };
    *current_slot().lock().expect("performance mutex poisoned") = Some(session.clone());
    info!(session_id = id, kind = ?kind, "[performance] session started");
    session
}

/// 更新当前会话阶段；session_id 不匹配时忽略，避免旧事件覆盖新会话。
pub fn update_phase(session_id: u64, phase: PerformancePhase) -> Option<PerformanceSession> {
    let mut guard = current_slot().lock().expect("performance mutex poisoned");
    let session = guard.as_mut()?;
    if session.id != session_id {
        debug!(
            current_id = session.id,
            stale_id = session_id,
            "[performance] ignored stale phase update"
        );
        return None;
    }
    session.phase = phase;
    session.updated_at_ms = now_ms();
    session.error = None;
    Some(session.clone())
}

/// 标记当前会话失败，保留错误信息供诊断。
pub fn fail_performance(session_id: u64, error: impl Into<String>) -> Option<PerformanceSession> {
    let mut guard = current_slot().lock().expect("performance mutex poisoned");
    let session = guard.as_mut()?;
    if session.id != session_id {
        return None;
    }
    session.phase = PerformancePhase::Failed;
    session.updated_at_ms = now_ms();
    session.error = Some(error.into());
    Some(session.clone())
}

/// 停止当前表现会话；session_id 不匹配时忽略。
pub fn stop_performance(session_id: u64, reason: impl AsRef<str>) -> Option<PerformanceSession> {
    let mut guard = current_slot().lock().expect("performance mutex poisoned");
    let Some(session) = guard.as_ref() else {
        return None;
    };
    if session.id != session_id {
        debug!(
            current_id = session.id,
            stale_id = session_id,
            reason = reason.as_ref(),
            "[performance] ignored stale stop"
        );
        return None;
    }
    let stopped = guard.take();
    info!(
        session_id,
        reason = reason.as_ref(),
        "[performance] session stopped"
    );
    stopped
}

/// 返回当前表现会话快照。
pub fn current_performance() -> Option<PerformanceSession> {
    current_slot()
        .lock()
        .expect("performance mutex poisoned")
        .clone()
}

/// 是否有会阻塞 bubble / screenshot 等后台表现的会话。
pub fn is_performing() -> bool {
    current_performance()
        .map(|session| session.phase.blocks_background_work())
        .unwrap_or(false)
}

/// 是否有需要暂停定时截图观察的表现会话。
pub fn blocks_screenshot_observation() -> bool {
    current_performance()
        .map(|session| {
            session.phase.blocks_background_work() && session.kind.blocks_screenshot_observation()
        })
        .unwrap_or(false)
}

#[cfg(test)]
pub fn reset_for_tests() {
    *current_slot().lock().expect("performance mutex poisoned") = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("performance test lock poisoned")
    }

    #[test]
    fn performance_blocks_background_work_until_stopped() {
        let _guard = test_lock();
        reset_for_tests();
        let session = start_performance(PerformanceKind::ChoreographedDance);
        assert!(is_performing());
        assert!(blocks_screenshot_observation());

        update_phase(session.id, PerformancePhase::Active);
        assert!(is_performing());
        assert!(blocks_screenshot_observation());

        stop_performance(session.id, "finished");
        assert!(!is_performing());
        assert!(!blocks_screenshot_observation());
    }

    #[test]
    fn music_reactive_dance_does_not_block_screenshot_observation() {
        let _guard = test_lock();
        reset_for_tests();
        let session = start_performance(PerformanceKind::MusicReactiveDance);
        update_phase(session.id, PerformancePhase::Active);

        assert!(is_performing());
        assert!(!blocks_screenshot_observation());
    }

    #[test]
    fn stale_session_updates_are_ignored() {
        let _guard = test_lock();
        reset_for_tests();
        let first = start_performance(PerformanceKind::ChoreographedDance);
        let second = start_performance(PerformanceKind::MusicReactiveDance);

        assert!(update_phase(first.id, PerformancePhase::Active).is_none());
        assert!(stop_performance(first.id, "old").is_none());
        assert_eq!(current_performance().unwrap().id, second.id);
    }
}
