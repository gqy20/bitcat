use crate::memory::MemoryConfig;
use crate::screen_summary::ScreenSummaryConfig;
use serde::{Deserialize, Serialize};
use std::fs;

// ---- 默认值 ----

const DEFAULT_AGENT_PREAMBLE: &str = r#"你是 8Bit，一只住在电脑屏幕上的像素风小猫助手。

性格特点：
- 活泼好奇，喜欢用 emoji
- 偶尔调皮，但做事靠谱
- 回答简洁，不说废话
- 用中文交流

你通过手柄和用户交互，可以帮用户：
- 启动程序、执行命令
- 查时间、读文件
- 闲聊、讲笑话、提醒事项

回答时保持角色感，像一只懂技术的猫。"#;

const DEFAULT_VISION_PROMPT: &str = r#"你是 8Bit，一只住在电脑屏幕上的像素风小猫助手。你刚刚看了一眼主人的屏幕。

严格遵守以下规则：
1. 如果你无法看清文字、标签、文件名，必须说"看不清"，绝对不要猜测或编造
2. 对于模糊的图标，只描述颜色和形状，用"看起来像是"而非"就是"
3. 不要编造任何具体的名称、数字、文字内容
4. 与其编造细节，不如诚实说"这个太小了喵~我看不太清"
5. 回复控制在 80 字以内，语气活泼可爱，像猫的视角

请描述你看到的屏幕内容。"#;

const DEFAULT_VISION_PROMPT_MULTI: &str = r#"
注意：这张截图是多显示器拼接的，内容从左到右排列。
左边通常是副屏，右边通常是主屏。请分别描述各屏的内容。"#;

// ---- 统一配置结构 ----

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

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PromptsConfig {
    #[serde(default)]
    pub agent: AgentPromptConfig,
    #[serde(default)]
    pub vision: VisionPromptConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub screen_summary: ScreenSummaryConfig,
}

fn default_agent_preamble() -> String {
    DEFAULT_AGENT_PREAMBLE.to_string()
}
fn default_vision_prompt() -> String {
    DEFAULT_VISION_PROMPT.to_string()
}
fn default_vision_prompt_multi() -> String {
    DEFAULT_VISION_PROMPT_MULTI.to_string()
}

impl Default for AgentPromptConfig {
    fn default() -> Self {
        Self {
            preamble: default_agent_preamble(),
        }
    }
}

impl Default for VisionPromptConfig {
    fn default() -> Self {
        Self {
            prompt: default_vision_prompt(),
            prompt_multi: default_vision_prompt_multi(),
        }
    }
}

impl PromptsConfig {
    /// 加载 prompts.yml：exe 同目录 → CWD → 编译时嵌入的默认值
    pub fn load() -> Self {
        const DEFAULT_YML: &str = include_str!("../../prompts.yml");
        let content = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("prompts.yml")))
            .filter(|p| p.exists())
            .and_then(|p| fs::read_to_string(p).ok())
            .or_else(|| fs::read_to_string("prompts.yml").ok())
            .unwrap_or_else(|| DEFAULT_YML.to_string());
        match serde_yaml::from_str::<PromptsConfig>(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(error = %e, "解析 prompts.yml 失败，使用默认提示词");
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agent_preamble_contains_key_phrases() {
        let cfg = AgentPromptConfig::default();
        assert!(!cfg.preamble.is_empty());
        assert!(cfg.preamble.contains("8Bit"));
        assert!(cfg.preamble.contains("猫"));
    }

    #[test]
    fn test_default_vision_prompt_anti_hallucination() {
        let cfg = VisionPromptConfig::default();
        assert!(cfg.prompt.contains("不要"));
        assert!(cfg.prompt.contains("编造"));
        assert!(cfg.prompt.contains("看不清"));
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
        assert!(!cfg.vision.prompt.is_empty()); // 默认值
        assert!(!cfg.vision.prompt_multi.is_empty());
    }

    #[test]
    fn test_empty_config_all_defaults() {
        let cfg: PromptsConfig = serde_yaml::from_str("{}").unwrap();
        assert!(cfg.agent.preamble.contains("8Bit"));
        assert!(cfg.vision.prompt.contains("8Bit"));
        assert_eq!(cfg.memory.max_entries, 20);
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
        assert_eq!(cfg.memory.max_entries, 20);
        assert_eq!(cfg.memory.max_context_chars, 1500);
    }
}
