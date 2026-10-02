//! 对话正文完成后的有序后台收尾。
//! 单一有界 worker 执行情绪提取、长期记忆候选事务和空闲画像聚合，避免阻塞下一轮输入。
//! 它通过请求生命周期锁发布仍有效的反应，并与 SharedChatCore 和 core 的持久化事务交互。

use crate::bubble::ChatRequest;
use crate::gamepad::{SharedChatCancel, SharedChatCore};
use crate::pet_event_bus::SharedPetEventBus;
use bitcat_core::agent_reaction::{extract_agent_reaction, fallback_agent_reaction};
use bitcat_core::ai_config::AiConfig;
use bitcat_core::memory::{LongTermEntry, LongTermMemory, ProfileStore};
use bitcat_core::pet_event::PetEvent;
use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tracing::{info, warn};

const QUEUE_CAPACITY: usize = 16;
/// 从正文完成计时，覆盖原有八秒提取超时，超过十秒的情绪只进诊断，不突然打扰。
const REACTION_DISPLAY_MAX_AGE: Duration = Duration::from_secs(10);
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(8);
const AGGREGATION_TIMEOUT: Duration = Duration::from_secs(15);

/// 单 worker 的失败重试期限，沿用配置间隔；未达阈值和成功提交不占重试窗口。
#[derive(Default)]
struct AggregationRetry {
    retry_not_before: Option<Instant>,
}

impl AggregationRetry {
    fn ready(&self, now: Instant) -> bool {
        self.retry_not_before.is_none_or(|deadline| now >= deadline)
    }

    fn begin_attempt(&mut self, now: Instant, interval: Duration) -> bool {
        if interval.is_zero() {
            self.retry_not_before = None;
            return false;
        }
        if !self.ready(now) {
            return false;
        }
        let Some(deadline) = now.checked_add(interval) else {
            return false;
        };
        self.retry_not_before = Some(deadline);
        true
    }

    fn complete(&mut self) {
        self.retry_not_before = None;
    }
}

/// 已完成正文的收尾任务；请求元数据不会从随后变化的当前会话重新获取。
pub(crate) struct ReactionJob {
    /// 已完成正文的真实请求原文和编号。
    pub request: ChatRequest,
    /// 已经显示给用户的完整回复。
    pub reply: String,
    /// 该轮实际使用的模型配置，不从稍后的会话重新取得。
    pub config: AiConfig,
    /// 该轮工具事实，供结构化收尾参考。
    pub tool_summaries: Vec<String>,
    /// 沿用该轮长期记忆配置的容量约束。
    pub max_entries: usize,
    /// 正文完成时刻，用于限制情绪出现的延迟。
    pub completed_at: Instant,
}

/// 单个有界收尾队列，receiver 只能被启动一次。
pub(crate) struct SharedChatReaction {
    sender: SyncSender<ReactionJob>,
    receiver: Mutex<Option<Receiver<ReactionJob>>>,
}

impl SharedChatReaction {
    pub(crate) fn new() -> Self {
        Self::with_capacity(QUEUE_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        Self {
            sender,
            receiver: Mutex::new(Some(receiver)),
        }
    }

    /// 收尾为可选步骤；饱和时记录诊断，不阻塞或撤回已经完成的正文。
    pub(crate) fn submit(&self, job: ReactionJob) -> bool {
        match self.sender.try_send(job) {
            Ok(()) => true,
            Err(TrySendError::Full(job)) => {
                warn!(
                    request_id = job.request.request_id,
                    capacity = QUEUE_CAPACITY,
                    "chat reaction queue full; optional closing skipped"
                );
                false
            }
            Err(TrySendError::Disconnected(job)) => {
                warn!(
                    request_id = job.request.request_id,
                    "chat reaction worker unavailable"
                );
                false
            }
        }
    }
}

/// 启动唯一 worker；正文线程不创建提取线程、不等待提取或画像网络请求。
pub(crate) fn spawn_worker(app: AppHandle) {
    let state: State<SharedChatReaction> = app.state();
    let receiver = state
        .receiver
        .lock()
        .ok()
        .and_then(|mut receiver| receiver.take());
    let Some(receiver) = receiver else {
        return;
    };
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(error) => {
                warn!(%error, "chat reaction runtime unavailable");
                return;
            }
        };
        let mut aggregation_retry = AggregationRetry::default();
        run_worker_loop(
            receiver,
            |job| finish_reaction(&app, &rt, job),
            || aggregate_if_due(&app, &rt, &mut aggregation_retry),
            crate::shutdown::is_requested,
        );
    });
}

fn run_worker_loop(
    receiver: Receiver<ReactionJob>,
    mut close: impl FnMut(ReactionJob),
    mut idle: impl FnMut(),
    stopped: impl Fn() -> bool,
) {
    while !stopped() {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(job) => close(job),
            Err(RecvTimeoutError::Timeout) => idle(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[derive(Debug)]
enum ProfileCommitError {
    Sources(String),
    Profile(String),
    Cache(String),
}

/// 先提交已验证的来源标记，再保存画像；缓存仅在两个持久化步骤均成功后更新。
fn commit_staged_profile<T>(
    cached: &Mutex<ProfileStore>,
    staged: ProfileStore,
    commit_sources: impl FnOnce() -> Result<T, String>,
    save_profile: impl FnOnce(&ProfileStore) -> Result<(), String>,
) -> Result<T, ProfileCommitError> {
    let sources = commit_sources().map_err(ProfileCommitError::Sources)?;
    save_profile(&staged).map_err(ProfileCommitError::Profile)?;
    *cached
        .lock()
        .map_err(|e| ProfileCommitError::Cache(e.to_string()))? = staged;
    Ok(sources)
}

fn publish_timely(
    cancel: &SharedChatCancel,
    request_id: u64,
    completed_at: Instant,
    publish: impl FnOnce(),
) -> bool {
    cancel
        .publish_if_current(request_id, || {
            let elapsed = completed_at.elapsed();
            if elapsed > REACTION_DISPLAY_MAX_AGE {
                info!(
                    request_id,
                    age_ms = elapsed.as_millis(),
                    "chat reaction display expired"
                );
                return false;
            }
            publish();
            true
        })
        .unwrap_or(false)
}

fn finish_reaction(app: &AppHandle, rt: &tokio::runtime::Runtime, job: ReactionJob) {
    let cancel: State<SharedChatCancel> = app.state();
    let request_id = job.request.request_id;
    if cancel.is_cancelled(request_id) || crate::shutdown::is_requested() {
        return;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(cancel.run_until_cancelled(request_id, async {
            tokio::time::timeout(
                EXTRACTION_TIMEOUT,
                extract_agent_reaction(
                    &job.config,
                    &job.request.user_text,
                    &job.reply,
                    &job.tool_summaries,
                ),
            )
            .await
        }))
    }));
    let reaction = match result {
        Ok(Some(Ok(Ok(reaction)))) => reaction,
        Ok(Some(Ok(Err(error)))) => fallback_agent_reaction(&job.reply, &error),
        Ok(Some(Err(_))) => fallback_agent_reaction(&job.reply, "AgentReaction timed out"),
        Ok(None) => return,
        Err(_) => fallback_agent_reaction(&job.reply, "AgentReaction panicked"),
    };
    if cancel.is_cancelled(request_id) || crate::shutdown::is_requested() {
        return;
    }

    let shown = publish_timely(&cancel, request_id, job.completed_at, || {
        let bus: State<SharedPetEventBus> = app.state();
        bus.emit(
            app,
            PetEvent::React {
                mood: reaction.mood,
                speech: (!reaction.speech.is_empty()).then(|| reaction.speech.clone()),
                ttl_ms: None,
            },
        );
    });
    if !shown {
        info!(
            request_id,
            latest_request_id = cancel.latest_request_id(),
            "chat reaction display skipped"
        );
    }
    if reaction.memory_candidates.is_empty() {
        return;
    }
    let update = LongTermMemory::update_latest(|store| {
        if cancel.is_cancelled(request_id) || crate::shutdown::is_requested() {
            return Err("closing request cancelled".to_string());
        }
        for candidate in &reaction.memory_candidates {
            store.record_candidate(
                candidate,
                &job.request.user_text,
                &job.reply,
                job.max_entries,
            );
        }
        Ok(())
    });
    match update {
        Ok((store, ())) => {
            let core: State<SharedChatCore> = app.state();
            if let Ok(mut cached) = core.long_term.lock() {
                *cached = store;
            }
            bitcat_core::points::award(
                bitcat_core::points::PointsEventKind::MemoryCreated,
                Some(&format!("{} 条", reaction.memory_candidates.len())),
            );
        }
        Err(error) => warn!(%error, request_id, "chat reaction memory transaction failed"),
    }
}

fn aggregate_if_due(app: &AppHandle, rt: &tokio::runtime::Runtime, retry: &mut AggregationRetry) {
    let now = Instant::now();
    // 冷却期间连配置/记录读取也避让，读取错误不会在每次 idle 时刷日志。
    if !retry.ready(now) {
        return;
    }
    let core: State<SharedChatCore> = app.state();
    let prompts = bitcat_core::prompts::PromptsConfig::load();
    let interval = Duration::from_secs(prompts.memory_v2.aggregation_interval_min as u64 * 60);
    // 零间隔表示关闭；开始读取前登记期限，所有失败早退都保留它。
    if !retry.begin_attempt(now, interval) {
        return;
    }
    let store = match LongTermMemory::load_checked() {
        Ok(store) => store,
        Err(error) => {
            warn!(%error, "profile aggregation memory read failed");
            return;
        }
    };
    let entries: Vec<LongTermEntry> = store.unaggregated_entries().into_iter().cloned().collect();
    let profile = match core.profile.lock() {
        Ok(profile) => profile.profile_text.clone(),
        Err(_) => return,
    };
    let elapsed = match core.last_aggregation.lock() {
        Ok(last) => last.elapsed(),
        Err(_) => return,
    };
    if entries.is_empty() || !(entries.len() >= 20 || (!profile.is_empty() && elapsed >= interval))
    {
        retry.complete();
        return;
    }
    let config = match AiConfig::load() {
        Ok(config) => config,
        Err(error) => {
            warn!(%error, "profile aggregation configuration unavailable");
            return;
        }
    };
    let refs = entries.iter().collect::<Vec<_>>();
    let result = catch_unwind(AssertUnwindSafe(|| {
        rt.block_on(async {
            tokio::time::timeout(
                AGGREGATION_TIMEOUT,
                bitcat_core::memory::aggregate_profile(
                    &refs,
                    &profile,
                    &config,
                    &prompts.aggregation.prompt,
                ),
            )
            .await
        })
    }));
    let patch = match result {
        Ok(Ok(Ok(patch))) => patch,
        Ok(Ok(Err(error))) => {
            warn!(%error, "profile aggregation failed");
            return;
        }
        Ok(Err(_)) => {
            warn!("profile aggregation timed out");
            return;
        }
        Err(_) => {
            warn!("profile aggregation panicked");
            return;
        }
    };
    if crate::shutdown::is_requested() {
        return;
    }
    let mut staged = match core.profile.lock() {
        Ok(profile) => profile.clone(),
        Err(error) => {
            warn!(%error, "profile aggregation cache unavailable");
            return;
        }
    };
    if let Err(error) = staged.apply_patch(&patch, &refs) {
        bitcat_core::memory::record_profile_aggregation_diagnostic(
            "profile_patch_rejected",
            Some(&error),
            &staged,
            &refs,
            Some(&patch),
        );
        warn!(%error, "profile aggregation patch rejected");
        return;
    }
    let ids: HashSet<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    let updated = commit_staged_profile(
        &core.profile,
        staged.clone(),
        || {
            LongTermMemory::update_latest(|latest| {
                if crate::shutdown::is_requested() {
                    return Err("application shutting down".into());
                }
                if entries.iter().any(|entry| {
                    !latest
                        .entries
                        .iter()
                        .any(|current| current.id == entry.id && !current.deleted)
                }) {
                    return Err("aggregation source was deleted during analysis".into());
                }
                for entry in &mut latest.entries {
                    if !entry.deleted && ids.contains(entry.id.as_str()) {
                        entry.aggregated = true;
                    }
                }
                Ok(())
            })
        },
        |profile| profile.save(),
    );
    match updated {
        Ok((store, ())) => {
            if let Ok(mut cached) = core.long_term.lock() {
                *cached = store;
            }
            if let Ok(mut last) = core.last_aggregation.lock() {
                *last = Instant::now();
            }
            retry.complete();
            bitcat_core::memory::record_profile_aggregation_diagnostic(
                "profile_patch_applied",
                None,
                &staged,
                &refs,
                Some(&patch),
            );
            info!(
                count = entries.len(),
                "profile aggregation completed in closing worker"
            );
        }
        Err(ProfileCommitError::Sources(error)) => {
            warn!(%error, "profile aggregation source commit failed; profile unchanged");
        }
        Err(ProfileCommitError::Profile(error)) => {
            // 来源已经落盘但画像尚未提交。重新标记原快照，允许后续重试，不复原删除记录。
            bitcat_core::memory::record_profile_aggregation_diagnostic(
                "profile_save_failed_after_source_commit",
                Some(&error),
                &staged,
                &refs,
                Some(&patch),
            );
            warn!(%error, "profile aggregation partial commit; profile cache unchanged");
            match LongTermMemory::update_latest(|latest| {
                for entry in &mut latest.entries {
                    if !entry.deleted && ids.contains(entry.id.as_str()) {
                        entry.aggregated = false;
                    }
                }
                Ok(())
            }) {
                Ok((store, ())) => {
                    if let Ok(mut cached) = core.long_term.lock() {
                        *cached = store;
                    }
                }
                Err(rollback_error) => {
                    warn!(%rollback_error, %error, "profile aggregation source reset failed after partial commit")
                }
            }
        }
        Err(ProfileCommitError::Cache(error)) => {
            warn!(%error, "profile aggregation persisted but cache update failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bubble::ChatSource;
    use std::sync::{Arc, Barrier};

    #[test]
    fn aggregation_zero_interval_disables_attempts() {
        let now = Instant::now();
        let mut retry = AggregationRetry::default();
        assert!(!retry.begin_attempt(now, Duration::ZERO));
        assert_eq!(retry.retry_not_before, None);
        assert!(retry.begin_attempt(now, Duration::from_secs(60)));
        assert!(!retry.begin_attempt(now, Duration::ZERO));
        assert_eq!(retry.retry_not_before, None);
    }

    #[test]
    fn aggregation_failure_waits_until_configured_retry_deadline() {
        let now = Instant::now();
        let interval = Duration::from_secs(60);
        let mut retry = AggregationRetry::default();
        assert!(retry.begin_attempt(now, interval));
        // 失败/超时直接返回，不撤销开始尝试时登记的期限。
        assert!(!retry.ready(now + Duration::from_secs(15)));
        assert!(!retry.begin_attempt(now + Duration::from_secs(59), interval));
        assert_eq!(retry.retry_not_before, Some(now + interval));
        assert!(retry.begin_attempt(now + interval, interval));
        assert_eq!(retry.retry_not_before, Some(now + interval + interval));
    }

    #[test]
    fn aggregation_success_or_not_due_clears_retry_window() {
        let now = Instant::now();
        let interval = Duration::from_secs(60);
        let mut retry = AggregationRetry::default();
        assert!(retry.begin_attempt(now, interval));
        retry.complete();
        assert!(retry.ready(now));
        assert!(retry.begin_attempt(now, interval));
        retry.complete();
        assert_eq!(retry.retry_not_before, None);
    }

    fn job(request_id: u64) -> ReactionJob {
        ReactionJob {
            request: ChatRequest {
                request_id,
                user_text: "当前问题".into(),
                source: ChatSource::Text,
            },
            reply: "当前回复".into(),
            config: AiConfig {
                api_key: "test".into(),
                base_url: "http://localhost".into(),
                model: "test".into(),
            },
            tool_summaries: Vec::new(),
            max_entries: 0,
            completed_at: Instant::now(),
        }
    }

    #[test]
    fn closing_queue_is_bounded_and_preserves_order() {
        let state = SharedChatReaction::with_capacity(2);
        assert!(state.submit(job(1)));
        assert!(state.submit(job(2)));
        assert!(!state.submit(job(3)));
        let receiver = state.receiver.lock().unwrap().take().unwrap();
        assert_eq!(receiver.try_recv().unwrap().request.request_id, 1);
        assert_eq!(receiver.try_recv().unwrap().request.request_id, 2);
    }

    #[test]
    fn second_reply_finishes_while_first_closing_is_blocked() {
        use crate::gamepad::{accept_chat, consume_next, SharedPendingChat};
        use std::sync::atomic::{AtomicBool, Ordering};
        let pending = SharedPendingChat::new();
        let cancel = SharedChatCancel::new();
        let closing = SharedChatReaction::with_capacity(4);
        let receiver = closing.receiver.lock().unwrap().take().unwrap();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stop = stopped.clone();
        let worker = std::thread::spawn(move || {
            run_worker_loop(
                receiver,
                |job| {
                    if job.request.request_id == 1 {
                        started_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                    } else {
                        worker_stop.store(true, Ordering::SeqCst);
                    }
                },
                || {},
                || worker_stop.load(Ordering::SeqCst),
            )
        });
        accept_chat(&pending, &cancel, "第一句".into(), ChatSource::Text, |_| {}).unwrap();
        assert!(consume_next(&pending, &cancel, |request| {
            assert!(closing.submit(job(request.request_id)));
        }));
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("第一轮收尾已开始但仍未完成");
        let second = accept_chat(
            &pending,
            &cancel,
            "第二句".into(),
            ChatSource::Voice,
            |_| {},
        )
        .unwrap()
        .request_id;
        let completed_second = AtomicBool::new(false);
        assert!(consume_next(&pending, &cancel, |request| {
            assert_eq!(request.request_id, second);
            assert!(!stopped.load(Ordering::SeqCst));
            completed_second.store(true, Ordering::SeqCst);
            assert!(closing.submit(job(request.request_id)));
        }));
        assert!(completed_second.load(Ordering::SeqCst));
        release_tx.send(()).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn source_commit_failure_never_saves_or_updates_profile() {
        let cached = Mutex::new(profile("原画像"));
        let result = commit_staged_profile(
            &cached,
            profile("新画像"),
            || Err::<(), _>("JSONL write failed".into()),
            |_| panic!("来源提交失败时不能保存画像"),
        );
        assert!(matches!(result, Err(ProfileCommitError::Sources(_))));
        assert_eq!(cached.lock().unwrap().profile_text, "原画像");
    }

    #[test]
    fn profile_save_failure_keeps_old_cache_and_reports_partial_commit() {
        let cached = Mutex::new(profile("原画像"));
        let result = commit_staged_profile(
            &cached,
            profile("新画像"),
            || Ok(()),
            |_| Err("profile write failed".into()),
        );
        assert!(matches!(result, Err(ProfileCommitError::Profile(_))));
        assert_eq!(cached.lock().unwrap().profile_text, "原画像");
    }

    fn profile(text: &str) -> ProfileStore {
        ProfileStore {
            facts: Vec::new(),
            revision: 1,
            profile_text: text.into(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn newer_request_cancel_and_display_age_suppress_old_reactions() {
        let cancel = SharedChatCancel::new();
        let first = cancel.begin_chat();
        assert!(publish_timely(&cancel, first, Instant::now(), || {}));
        let second = cancel.begin_chat();
        assert!(!publish_timely(&cancel, first, Instant::now(), || panic!(
            "旧情绪不能覆盖新问题"
        )));
        assert!(!publish_timely(
            &cancel,
            second,
            Instant::now() - REACTION_DISPLAY_MAX_AGE - Duration::from_millis(1),
            || panic!("过期情绪不得展示")
        ));
        cancel.cancel_current();
        assert!(!publish_timely(&cancel, second, Instant::now(), || panic!(
            "取消的情绪不得展示"
        )));
    }

    #[test]
    fn request_acceptance_cannot_split_reaction_check_and_publish() {
        let cancel = Arc::new(SharedChatCancel::new());
        let first = cancel.begin_chat();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let publisher = {
            let cancel = cancel.clone();
            let entered = entered.clone();
            let release = release.clone();
            std::thread::spawn(move || {
                publish_timely(&cancel, first, Instant::now(), || {
                    entered.wait();
                    release.wait();
                    assert_eq!(cancel.latest_request_id(), first);
                })
            })
        };
        entered.wait();
        let next = {
            let cancel = cancel.clone();
            std::thread::spawn(move || cancel.begin_chat())
        };
        release.wait();
        assert!(publisher.join().unwrap());
        let second = next.join().unwrap();
        assert!(second > first);
        assert!(!publish_timely(&cancel, first, Instant::now(), || panic!(
            "新请求接受后不得发送旧情绪"
        )));
    }
}
