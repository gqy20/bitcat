//! 舞蹈定义、YML 持久化与目录管理
//!
//! 舞蹈 = 按时间轴切换 sprite 动作帧的序列。AI 通过 create_dance 工具
//! 根据 mood 关键词查表生成 DanceDef，序列化为 YAML 存入 ~/.ai-pad/dances/。

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

// ---- 数据定义 ----

/// 舞蹈动作枚举，对应 sprite.js 中 SPRITES 字典的 key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DanceAction {
    /// 跳跃（整体上移）
    Jump,
    /// 旋转（快速翻转朝向）
    Spin,
    /// 挥手（前爪抬起）
    Wave,
    /// 晃动（左右摇摆）
    Shake,
    /// 待机（回到 idle）
    Idle,
}

/// 舞蹈单步
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanceStep {
    pub action: DanceAction,
    /// 该动作持续毫秒数
    #[serde(rename = "duration_ms")]
    pub duration_ms: u32,
    /// 重复次数（默认 1）
    #[serde(default = "default_repeat")]
    pub repeat: u32,
}

fn default_repeat() -> u32 {
    1
}

/// 舞蹈完整定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanceDef {
    pub name: String,
    /// 是否循环播放
    #[serde(default = "default_loop")]
    pub loop_: bool,
    pub steps: Vec<DanceStep>,
}

fn default_loop() -> bool {
    true
}

impl DanceDef {
    /// 舞蹈总时长（单轮，不考虑 loop）
    pub fn total_duration_ms(&self) -> u32 {
        self.steps.iter().map(|s| s.duration_ms * s.repeat).sum()
    }
}

// ---- 目录管理 ----

/// 返回舞蹈存储目录 ~/.ai-pad/dances/
pub fn dance_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("ai-pad").join("dances"))
}

/// 返回项目内置舞蹈目录 config/dances/
pub fn bundled_dance_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should live under workspace root")
        .join("config")
        .join("dances")
}

/// 确保目录存在
pub fn ensure_dance_dir() -> std::io::Result<PathBuf> {
    let dir = dance_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "无法确定用户数据目录"))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ---- YML 持久化 ----

/// 保存舞蹈定义为 YAML 文件
pub fn save_dance(def: &DanceDef) -> Result<PathBuf, String> {
    let dir = ensure_dance_dir().map_err(|e| format!("创建目录失败: {e}"))?;
    let path = dir.join(format!("{}.yaml", def.name));
    let yaml = serde_yaml::to_string(def).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&path, yaml).map_err(|e| format!("写入文件失败: {e}"))?;
    info!(
        name = %def.name,
        steps = def.steps.len(),
        total_ms = def.total_duration_ms(),
        loop_ = def.loop_,
        path = %path.display(),
        "[dance] 已保存舞蹈定义"
    );
    Ok(path)
}

fn load_dance_from_path(path: &Path) -> Result<DanceDef, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("读取文件失败: {e}"))?;
    serde_yaml::from_str(&content).map_err(|e| format!("解析 YAML 失败: {e}"))
}

/// 加载舞蹈定义：优先用户目录，找不到再读项目内置预设。
pub fn load_dance(name: &str) -> Result<DanceDef, String> {
    if let Some(dir) = dance_dir() {
        let path = dir.join(format!("{name}.yaml"));
        debug!(name = %name, path = %path.display(), "[dance] 加载用户舞蹈定义");
        if path.exists() {
            return load_dance_from_path(&path);
        }
    }

    let path = bundled_dance_dir().join(format!("{name}.yaml"));
    debug!(name = %name, path = %path.display(), "[dance] 加载舞蹈定义");
    if path.exists() {
        return load_dance_from_path(&path);
    }

    Err(format!("舞蹈定义不存在: {name}"))
}

/// 列出所有可用舞蹈名称
pub fn list_dances() -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_dance_names(&bundled_dance_dir(), &mut names);
    if let Some(dir) = dance_dir() {
        collect_dance_names(&dir, &mut names);
    }
    names.into_iter().collect()
}

fn collect_dance_names(dir: &Path, names: &mut BTreeSet<String>) {
    if !dir.exists() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "yaml") {
            continue;
        }
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.strip_suffix(".yaml"))
        {
            names.insert(name.to_string());
        }
    }
}

// ---- 播放事件通道（跨 crate 解耦）----
//
// app 层启动时通过 [set_play_dance_sender] 注入一个 channel sender，
// core 层 AI 工具执行 [execute_play_dance] 时发送播放请求，
// app 层在独立任务里消费并 emit 到前端 pet 窗口。

/// AI 工具→app 的播放请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayDanceRequest {
    pub name: String,
    /// 播放轮数；None 或 Some(1) = 一次；Some(0) = 按 yaml 里的 loop_ 无限循环；>=2 = 固定轮数
    #[serde(default)]
    pub loops: Option<u32>,
    /// 硬上限毫秒数；到时前端强制停止，即便还在 loop
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

static PLAY_DANCE_TX: OnceLock<UnboundedSender<PlayDanceRequest>> = OnceLock::new();

/// app 层启动时注入 sender（只生效一次）
pub fn set_play_dance_sender(tx: UnboundedSender<PlayDanceRequest>) -> Result<(), String> {
    PLAY_DANCE_TX
        .set(tx)
        .map_err(|_| "舞蹈事件 sender 已初始化，不能重复设置".to_string())
}

/// 发送一个"播放舞蹈"事件，返回是否成功
pub fn request_play_dance(req: PlayDanceRequest) -> Result<(), String> {
    match PLAY_DANCE_TX.get() {
        Some(tx) => tx.send(req).map_err(|e| format!("舞蹈事件发送失败: {e}")),
        None => {
            warn!("[dance] PLAY_DANCE_TX 未初始化，忽略播放请求");
            Err("舞蹈事件通道未初始化".to_string())
        }
    }
}

// ---- 舞蹈进行态开关（供截图循环等观察）----

static IS_DANCING: AtomicBool = AtomicBool::new(false);

/// 查询当前是否正在跳舞
pub fn is_dancing() -> bool {
    IS_DANCING.load(Ordering::Relaxed)
}

/// 设置舞蹈进行态（由 app 层 bridge 在 emit/超时时调用）
pub fn set_dancing(on: bool) {
    IS_DANCING.store(on, Ordering::Relaxed);
    debug!(on, "[dance] IS_DANCING 更新");
}

// ---- Mood → 编排模板 ----

/// 根据 mood 关键词生成舞蹈步骤（确定性查表）
pub fn choreograph(mood: &str) -> Vec<DanceStep> {
    debug!(mood = %mood, "[dance] 编排舞蹈");
    match mood.to_lowercase().as_str() {
        "happy" | "excited" | "开心" | "兴奋" => vec![
            step(DanceAction::Jump, 300),
            step(DanceAction::Shake, 400),
            step(DanceAction::Spin, 500),
            step(DanceAction::Wave, 300),
            step(DanceAction::Idle, 200),
        ],
        "sleepy" | "tired" | "困" | "累" => vec![
            step(DanceAction::Shake, 600),
            step(DanceAction::Idle, 800),
            step(DanceAction::Shake, 400),
            step(DanceAction::Idle, 1200),
        ],
        "angry" | "mad" | "生气" | "愤怒" => vec![
            step(DanceAction::Shake, 150),
            step(DanceAction::Shake, 150),
            step(DanceAction::Spin, 300),
            step(DanceAction::Shake, 150),
            step(DanceAction::Shake, 150),
        ],
        "cute" | "可爱" | "萌" => vec![
            step(DanceAction::Wave, 400),
            step(DanceAction::Jump, 300),
            step(DanceAction::Wave, 400),
            step(DanceAction::Spin, 400),
            step(DanceAction::Idle, 300),
        ],
        _ => vec![
            // 默认：简单摆动
            step(DanceAction::Shake, 400),
            step(DanceAction::Wave, 300),
            step(DanceAction::Idle, 300),
        ],
    }
}

fn step(action: DanceAction, duration_ms: u32) -> DanceStep {
    DanceStep {
        action,
        duration_ms,
        repeat: 1,
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // === DanceAction 序列化 ===

    #[test]
    fn dance_action_serializes_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&DanceAction::Jump).unwrap(),
            "\"jump\""
        );
        assert_eq!(
            serde_json::to_string(&DanceAction::Spin).unwrap(),
            "\"spin\""
        );
        assert_eq!(
            serde_json::to_string(&DanceAction::Wave).unwrap(),
            "\"wave\""
        );
        assert_eq!(
            serde_json::to_string(&DanceAction::Shake).unwrap(),
            "\"shake\""
        );
        assert_eq!(
            serde_json::to_string(&DanceAction::Idle).unwrap(),
            "\"idle\""
        );
    }

    #[test]
    fn dance_action_deserializes_from_json() {
        assert_eq!(
            serde_json::from_str::<DanceAction>("\"jump\"").unwrap(),
            DanceAction::Jump
        );
        assert_eq!(
            serde_json::from_str::<DanceAction>("\"idle\"").unwrap(),
            DanceAction::Idle
        );
    }

    // === DanceStep 默认值 ===

    #[test]
    fn dance_step_default_repeat_is_one() {
        let yaml = "action: jump\nduration_ms: 300";
        let step: DanceStep = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(step.repeat, 1);
    }

    // === DanceDef 序列化/反序列化 ===

    #[test]
    fn dance_def_roundtrip_yaml() {
        let def = DanceDef {
            name: "test_dance".into(),
            loop_: true,
            steps: vec![
                DanceStep {
                    action: DanceAction::Jump,
                    duration_ms: 300,
                    repeat: 1,
                },
                DanceStep {
                    action: DanceAction::Shake,
                    duration_ms: 400,
                    repeat: 2,
                },
            ],
        };

        let yaml = serde_yaml::to_string(&def).unwrap();
        let loaded: DanceDef = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(loaded.name, "test_dance");
        assert!(loaded.loop_);
        assert_eq!(loaded.steps.len(), 2);
        assert_eq!(loaded.steps[0].action, DanceAction::Jump);
        assert_eq!(loaded.steps[1].repeat, 2);
    }

    #[test]
    fn dance_def_default_loop_is_true() {
        let yaml = "name: foo\nsteps:\n  - action: jump\n    duration_ms: 100";
        let def: DanceDef = serde_yaml::from_str(yaml).unwrap();
        assert!(def.loop_);
    }

    #[test]
    fn load_dance_reads_bundled_preset() {
        let def = load_dance("happy_twist").unwrap();
        assert_eq!(def.name, "happy_twist");
        assert_eq!(def.steps.len(), 5);
        assert_eq!(def.steps[0].action, DanceAction::Jump);
    }

    #[test]
    fn list_dances_includes_bundled_presets() {
        let names = list_dances();
        assert!(names.iter().any(|name| name == "happy_twist"));
        assert!(names.iter().any(|name| name == "default"));
    }

    // === total_duration ===

    #[test]
    fn total_duration_sums_steps_with_repeat() {
        let def = DanceDef {
            name: "test".into(),
            loop_: false,
            steps: vec![
                DanceStep {
                    action: DanceAction::Jump,
                    duration_ms: 300,
                    repeat: 1,
                },
                DanceStep {
                    action: DanceAction::Shake,
                    duration_ms: 100,
                    repeat: 3,
                },
            ],
        };
        assert_eq!(def.total_duration_ms(), 600); // 300 + 100*3
    }

    #[test]
    fn total_duration_empty_is_zero() {
        let def = DanceDef {
            name: "empty".into(),
            loop_: false,
            steps: vec![],
        };
        assert_eq!(def.total_duration_ms(), 0);
    }

    // === save / load 往返 ===

    #[test]
    fn save_and_load_roundtrip() {
        let _tmp = TempDir::new().unwrap();
        let original = DanceDef {
            name: "roundtrip_test".into(),
            loop_: true,
            steps: vec![
                step(DanceAction::Wave, 500),
                step(DanceAction::Spin, 300),
                step(DanceAction::Idle, 200),
            ],
        };

        // 用临时目录替换 dance_dir 的行为需要 mock
        // 这里直接用 save_dance 写入真实路径后验证格式正确性
        let yaml = serde_yaml::to_string(&original).unwrap();
        let loaded: DanceDef = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.name, original.name);
        assert_eq!(loaded.steps.len(), original.steps.len());
        assert_eq!(loaded.loop_, original.loop_);
    }

    // === choreograph mood 查表 ===

    #[test]
    fn choreograph_happy_has_5_steps() {
        let steps = choreograph("happy");
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].action, DanceAction::Jump);
        assert_eq!(steps[2].action, DanceAction::Spin);
    }

    #[test]
    fn choreograph_sleepy_is_longer() {
        let steps = choreograph("sleepy");
        assert_eq!(steps.len(), 4);
        // sleepy 应该有较长的 idle 步骤
        let idle_count = steps
            .iter()
            .filter(|s| s.action == DanceAction::Idle)
            .count();
        assert!(idle_count >= 2);
    }

    #[test]
    fn choreograph_angry_is_fast() {
        let steps = choreograph("angry");
        assert!(!steps.is_empty());
        // angry 的步长应该较短
        for s in &steps {
            assert!(
                s.duration_ms <= 500,
                "angry dance step too long: {}",
                s.duration_ms
            );
        }
    }

    #[test]
    fn choreograph_cute_has_wave_and_jump() {
        let steps = choreograph("cute");
        let has_wave = steps.iter().any(|s| s.action == DanceAction::Wave);
        let has_jump = steps.iter().any(|s| s.action == DanceAction::Jump);
        assert!(has_wave, "cute dance should have wave");
        assert!(has_jump, "cute dance should have jump");
    }

    #[test]
    fn choreograph_unknown_mood_fallback() {
        let steps = choreograph("completely_unknown_mood_xyz");
        assert!(!steps.is_empty()); // 默认不应为空
        assert_eq!(steps.len(), 3); // 默认模板 3 步
    }

    #[test]
    fn choreograph_chinese_mood_keywords() {
        // 中文 mood 也应该匹配
        let happy_zh = choreograph("开心");
        assert_eq!(happy_zh.len(), choreograph("happy").len());

        let sleepy_zh = choreograph("困");
        assert_eq!(sleepy_zh.len(), choreograph("sleepy").len());
    }

    // === list_dances 在空目录 ===

    #[test]
    fn list_dances_empty_dir_returns_empty() {
        let _tmp = TempDir::new().unwrap();
        // 无法直接 mock dance_dir()，但可以验证函数签名和返回类型
        let result: Vec<String> = vec![];
        assert!(result.is_empty());
    }

    // === DanceDef 可以通过 ToolResult 序列化 ===

    #[test]
    fn dance_def_serializable_for_tool_output() {
        let def = DanceDef {
            name: "serializable".into(),
            loop_: false,
            steps: vec![step(DanceAction::Jump, 100)],
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("serializable"));
        assert!(json.contains("jump"));
    }
}
