use crate::memory::MemoryConfig;
use crate::screen_summary::ScreenSummaryConfig;
use serde::{Deserialize, Serialize};
use std::fs;

// ---- 内嵌默认值（唯一来源，与 config/prompts.yml 保持同步） ----

const EMBEDDED_YML: &str = include_str!("../../config/prompts.yml");

/// 从内嵌 YAML 解析完整配置，再提取对应字段作为默认值。
/// 这确保 Default/serde(default) 始终与编译嵌入的 YAML 一致，不会出现过时 const。
fn embedded_default<T: for<'de> Deserialize<'de>>() -> T {
    serde_yaml::from_str(EMBEDDED_YML).expect("内嵌 config/prompts.yml 损坏")
}

// ---- 数据结构 ----

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentPromptConfig {
    #[serde(default = "default_agent_preamble")]
    pub preamble: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VisionPromptConfig {
    #[serde(default = "default_vision_prompt")]
    pub prompt: String,
    #[serde(default = "default_vision_prompt_multi")]
    pub prompt_multi: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CameraPromptConfig {
    #[serde(default = "default_camera_prompt")]
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryV2Config {
    #[serde(default = "default_long_term_max")]
    pub long_term_max_entries: usize,
    #[serde(default = "default_retrieve_budget")]
    pub retrieve_budget_chars: usize,
    #[serde(default = "default_aggregation_interval")]
    pub aggregation_interval_min: u32,
}

fn default_long_term_max() -> usize {
    200
}
fn default_retrieve_budget() -> usize {
    10_000
}
fn default_aggregation_interval() -> u32 {
    24
}

impl Default for MemoryV2Config {
    fn default() -> Self {
        embedded_default()
    }
}

/// 记忆聚合提示词配置（来自 config/prompts.yml 的 aggregation 段）
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AggregationConfig {
    #[serde(default = "default_aggregation_prompt")]
    pub prompt: String,
}

fn default_aggregation_prompt() -> String {
    embedded_default::<PromptsConfig>().aggregation.prompt
}

/// 提醒到期后的 AI 文案润色提示词配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReminderPersonalizerPromptConfig {
    #[serde(default = "default_reminder_personalizer_preamble")]
    pub preamble: String,
}

fn default_reminder_personalizer_preamble() -> String {
    embedded_default::<PromptsConfig>()
        .reminder_personalizer
        .preamble
}

impl Default for ReminderPersonalizerPromptConfig {
    fn default() -> Self {
        Self {
            preamble: default_reminder_personalizer_preamble(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PromptsConfig {
    #[serde(default)]
    pub agent: AgentPromptConfig,
    #[serde(default)]
    pub vision: VisionPromptConfig,
    #[serde(default)]
    pub camera: CameraPromptConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub memory_v2: MemoryV2Config,
    #[serde(default)]
    pub screen_summary: ScreenSummaryConfig,
    #[serde(default)]
    pub aggregation: AggregationConfig,
    #[serde(default)]
    pub reminder_personalizer: ReminderPersonalizerPromptConfig,
}

fn default_agent_preamble() -> String {
    embedded_default::<PromptsConfig>().agent.preamble
}
fn default_vision_prompt() -> String {
    embedded_default::<PromptsConfig>().vision.prompt
}
fn default_vision_prompt_multi() -> String {
    embedded_default::<PromptsConfig>().vision.prompt_multi
}
fn default_camera_prompt() -> String {
    embedded_default::<PromptsConfig>().camera.prompt
}

impl Default for AgentPromptConfig {
    fn default() -> Self {
        embedded_default()
    }
}

impl Default for VisionPromptConfig {
    fn default() -> Self {
        embedded_default()
    }
}

impl Default for CameraPromptConfig {
    fn default() -> Self {
        embedded_default()
    }
}

impl PromptsConfig {
    /// 加载 config/prompts.yml：exe 同目录/config/ → CWD/config/ → 编译时嵌入的默认值
    pub fn load() -> Self {
        let content = load_prompts_content();
        match serde_yaml::from_str::<PromptsConfig>(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(error = %e, "解析 config/prompts.yml 失败，使用默认提示词");
                Self::default()
            }
        }
    }

    /// 序列化写回 config/prompts.yml（会覆盖注释，保存前自动备份 `.bak`）。
    pub fn save(&self) -> Result<(), String> {
        let target = resolve_prompts_path();
        if let Ok(old) = fs::read_to_string(&target) {
            let _ = fs::write(target.with_extension("yml.bak"), old);
        }
        let header = "# 由 BitCat 设置界面生成\n\
                      # 手动编辑仍然生效，但下次保存设置会覆盖注释\n\n";
        let body = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        fs::write(&target, format!("{header}{body}"))
            .map_err(|e| format!("写入 {:?} 失败: {e}", target))
    }

    /// 内置默认配置（解析嵌入的 config/prompts.yml）。
    /// 与 Default::default() 返回相同值——两者都来自同一内嵌 YAML。
    pub fn default_builtin() -> Self {
        embedded_default()
    }
}

const DEFAULT_PROMPTS_YML: &str = include_str!("../../config/prompts.yml");

fn load_prompts_content() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|dir| dir.join("config").join("prompts.yml"))
        })
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| fs::read_to_string("config/prompts.yml").ok())
        .unwrap_or_else(|| DEFAULT_PROMPTS_YML.to_string())
}

fn resolve_prompts_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|dir| dir.join("config").join("prompts.yml"))
        })
        .filter(|p| p.exists())
        .or_else(|| {
            let p = std::path::PathBuf::from("config/prompts.yml");
            if p.exists() { Some(p) } else { None }
        })
        .unwrap_or_else(|| std::path::PathBuf::from("config/prompts.yml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agent_preamble_contains_key_phrases() {
        let cfg = AgentPromptConfig::default();
        assert!(!cfg.preamble.is_empty());
        assert!(cfg.preamble.contains("BitCat"));
        assert!(cfg.preamble.contains("桌面 AI 伙伴"));
    }

    #[test]
    fn test_default_vision_prompt_anti_hallucination() {
        let cfg = VisionPromptConfig::default();
        // 断言来自 config/prompts.yml（内嵌 YAML），非旧 const
        assert!(cfg.prompt.contains("不要瞎猜"), "应包含反幻觉规则");
        assert!(cfg.prompt.contains("看不太清"), "应包含看不清指引");
        assert!(cfg.prompt.contains("120 字"), "应使用 YAML 中的 120 字限制");
    }

    #[test]
    fn test_full_config_snapshot() {
        let yaml = r#"
agent:
  preamble: "自定义 agent 提示词"
vision:
  prompt: "自定义视觉提示词"
  prompt_multi: "多屏提示"
"#;
        let cfg: PromptsConfig = serde_yaml::from_str(yaml).unwrap();
        insta::assert_yaml_snapshot!(cfg);
    }

    #[test]
    fn test_partial_config_uses_defaults() {
        let yaml = r#"
agent:
  preamble: "只有 agent"
"#;
        let cfg: PromptsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.agent.preamble, "只有 agent");
        assert!(!cfg.vision.prompt.is_empty()); // 默认值来自内嵌 YAML
        assert!(!cfg.vision.prompt_multi.is_empty());
    }

    #[test]
    fn test_empty_config_all_defaults() {
        let cfg: PromptsConfig = serde_yaml::from_str("{}").unwrap();
        assert!(cfg.agent.preamble.contains("BitCat"));
        assert!(cfg.vision.prompt.contains("BitCat"));
        assert_eq!(cfg.memory.max_entries, 0);
        assert_eq!(cfg.screen_summary.interval_min, 15);
        assert!(!cfg.screen_summary.prompt.is_empty());
    }

    #[test]
    fn test_partial_config_gets_default_memory() {
        let yaml = r#"
agent:
  preamble: "只有 agent"
"#;
        let cfg: PromptsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.agent.preamble, "只有 agent");
        assert_eq!(cfg.memory.max_entries, 0);
        assert_eq!(cfg.memory.max_context_chars, 20_000);
    }

    #[test]
    fn test_default_matches_builtin() {
        // Default 和 default_builtin 必须返回相同值（都来自内嵌 YAML）
        let d = PromptsConfig::default();
        let b = PromptsConfig::default_builtin();
        assert_eq!(d.agent.preamble, b.agent.preamble);
        assert_eq!(d.vision.prompt, b.vision.prompt);
        assert_eq!(d.vision.prompt_multi, b.vision.prompt_multi);
        assert_eq!(d.memory.max_entries, b.memory.max_entries);
        assert_eq!(
            d.reminder_personalizer.preamble,
            b.reminder_personalizer.preamble
        );
    }

    #[test]
    fn test_default_includes_reminder_personalizer_prompt() {
        let cfg = PromptsConfig::default();
        assert!(cfg.reminder_personalizer.preamble.contains("顶部通知文案"));
        assert!(cfg.reminder_personalizer.preamble.contains("tone"));
    }

    #[test]
    fn test_default_includes_dance_capability() {
        // 关键回归：默认 preamble 必须包含舞蹈工具说明（旧 const 缺失此内容）
        let cfg = AgentPromptConfig::default();
        assert!(
            cfg.preamble.contains("perform_dance"),
            "默认 preamble 应包含舞蹈工具说明（来自内嵌 YAML）"
        );
    }

    #[test]
    fn test_default_vision_has_correct_length_limit() {
        // 关键回归：默认 vision prompt 应使用 YAML 中的 120 字限制（旧 const 用了 80）
        let cfg = VisionPromptConfig::default();
        assert!(
            cfg.prompt.contains("120"),
            "默认 vision prompt 应使用 YAML 中的 120 字限制"
        );
        assert!(
            !cfg.prompt.contains("80 字"),
            "默认 vision prompt 不应包含旧的 80 字限制"
        );
    }
}
