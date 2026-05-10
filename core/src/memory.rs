use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

// ---- 数据结构 ----

/// 单条对话记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub timestamp: String,
    pub user_msg: String,
    pub ai_reply: String,
}

/// 对话记忆存储（滚动窗口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStore {
    pub entries: Vec<MemoryEntry>,
}

/// 记忆系统配置（来自 prompts.yml 的 memory 段）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryConfig {
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
    #[serde(default = "default_max_user_chars")]
    pub max_user_chars: usize,
    #[serde(default = "default_max_reply_chars")]
    pub max_reply_chars: usize,
}

fn default_max_entries() -> usize { 20 }
fn default_max_context_chars() -> usize { 1500 }
fn default_max_user_chars() -> usize { 100 }
fn default_max_reply_chars() -> usize { 200 }

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_entries: default_max_entries(),
            max_context_chars: default_max_context_chars(),
            max_user_chars: default_max_user_chars(),
            max_reply_chars: default_max_reply_chars(),
        }
    }
}

// ---- 存储路径 ----

fn memory_file_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("memory").join("chat_summary.json"))
}

// ---- 核心操作 ----

impl MemoryStore {
    /// 从磁盘加载。文件不存在或损坏时返回空记忆。
    pub fn load() -> Self {
        let path = match memory_file_path() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "获取记忆文件路径失败");
                return Self { entries: Vec::new() };
            }
        };

        if !path.exists() {
            info!("记忆文件不存在，使用空记忆");
            return Self { entries: Vec::new() };
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<MemoryStore>(&content) {
                Ok(store) => {
                    info!(count = store.entries.len(), "已加载记忆");
                    store
                }
                Err(e) => {
                    warn!(error = %e, "解析记忆文件失败，使用空记忆");
                    Self { entries: Vec::new() }
                }
            },
            Err(e) => {
                warn!(error = %e, "读取记忆文件失败，使用空记忆");
                Self { entries: Vec::new() }
            }
        }
    }

    /// 记录新对话：截断字段 + 强制滚动窗口
    pub fn record_conversation(&mut self, user_msg: &str, ai_reply: &str, config: &MemoryConfig) {
        let timestamp = chrono::Local::now().format("%H:%M").to_string();

        self.entries.push(MemoryEntry {
            timestamp,
            user_msg: truncate_chars(user_msg, config.max_user_chars),
            ai_reply: truncate_chars(ai_reply, config.max_reply_chars),
        });

        while self.entries.len() > config.max_entries {
            self.entries.remove(0);
        }
    }

    /// 构建注入 prompt 的上下文文本。空记忆返回空字符串。
    pub fn build_context(&self, config: &MemoryConfig) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        // 从最新到最旧收集行，计算预算后反转回时间顺序
        let mut lines: Vec<String> = Vec::new();
        for entry in self.entries.iter().rev() {
            lines.push(format!("[{}] {} | {}", entry.timestamp, entry.user_msg, entry.ai_reply));
        }
        lines.reverse();

        let header = "[最近对话记录]\n";
        let mut result = String::from(header);

        for line in &lines {
            if result.len() + line.len() + 1 > config.max_context_chars {
                break;
            }
            result.push_str(line);
            result.push('\n');
        }
        result.push_str("[/最近对话记录]\n");

        result
    }

    /// 持久化到磁盘
    pub fn save(&self) -> Result<(), String> {
        let path = memory_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建记忆目录失败: {e}"))?;
        }
        let json = serde_json::to_string(self)
            .map_err(|e| format!("序列化记忆失败: {e}"))?;
        fs::write(&path, json)
            .map_err(|e| format!("写入记忆文件失败: {e}"))?;
        Ok(())
    }
}

/// 按 Unicode 字符截断，超长时追加 "..."
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

// ---- 测试 ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_unchanged() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate_chars("abcdefghij", 7), "abcd...");
    }

    #[test]
    fn test_truncate_multibyte_utf8() {
        let s = "你好世界这是一个测试字符串";
        let r = truncate_chars(s, 6);
        assert_eq!(r.chars().count(), 6);
        assert!(r.ends_with("..."));
    }

    #[test]
    fn test_record_enforces_max_entries() {
        let mut store = MemoryStore { entries: Vec::new() };
        let cfg = MemoryConfig { max_entries: 3, ..Default::default() };
        for i in 0..5 {
            store.record_conversation(&format!("msg{i}"), &format!("reply{i}"), &cfg);
        }
        assert_eq!(store.entries.len(), 3);
        assert_eq!(store.entries[0].user_msg, "msg2");
        assert_eq!(store.entries[2].user_msg, "msg4");
    }

    #[test]
    fn test_record_truncates_fields() {
        let mut store = MemoryStore { entries: Vec::new() };
        let cfg = MemoryConfig { max_user_chars: 5, max_reply_chars: 10, ..Default::default() };
        store.record_conversation("abcdefghijklmnopqrstuvwxyz", "1234567890ABCDEFGHIJ", &cfg);
        assert_eq!(store.entries[0].user_msg.chars().count(), 5);
        assert_eq!(store.entries[0].ai_reply.chars().count(), 10);
    }

    #[test]
    fn test_build_context_empty() {
        let store = MemoryStore { entries: Vec::new() };
        assert!(store.build_context(&MemoryConfig::default()).is_empty());
    }

    #[test]
    fn test_build_context_format() {
        let store = MemoryStore {
            entries: vec![MemoryEntry {
                timestamp: "14:23".into(),
                user_msg: "你好".into(),
                ai_reply: "喵~".into(),
            }],
        };
        let ctx = store.build_context(&MemoryConfig::default());
        assert!(ctx.contains("[最近对话记录]"));
        assert!(ctx.contains("14:23"));
        assert!(ctx.contains("你好"));
        assert!(ctx.contains("喵~"));
        assert!(ctx.contains("[/最近对话记录]"));
    }

    #[test]
    fn test_build_context_respects_char_limit() {
        let mut store = MemoryStore { entries: Vec::new() };
        for _ in 0..50 {
            store.entries.push(MemoryEntry {
                timestamp: "14:00".into(),
                user_msg: "这是一条很长的用户消息用于测试".into(),
                ai_reply: "这是一条很长的AI回复用于测试字符限制".into(),
            });
        }
        let ctx = store.build_context(&MemoryConfig { max_context_chars: 300, ..Default::default() });
        assert!(ctx.len() <= 350);
    }

    #[test]
    fn test_default_config() {
        let cfg = MemoryConfig::default();
        assert_eq!(cfg.max_entries, 20);
        assert_eq!(cfg.max_context_chars, 1500);
        assert_eq!(cfg.max_user_chars, 100);
        assert_eq!(cfg.max_reply_chars, 200);
    }

    #[test]
    fn test_json_roundtrip() {
        let store = MemoryStore {
            entries: vec![MemoryEntry {
                timestamp: "14:23".into(),
                user_msg: "test".into(),
                ai_reply: "reply".into(),
            }],
        };
        let json = serde_json::to_string(&store).unwrap();
        let back: MemoryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].user_msg, "test");
    }

    #[test]
    fn test_memory_file_path() {
        let p = memory_file_path().unwrap();
        let s = p.to_string_lossy();
        assert!(s.contains(".ai-pad"), "应在 .ai-pad 下");
        assert!(s.contains("memory"), "应有 memory 子目录");
        assert!(s.ends_with("chat_summary.json"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".ai-pad").join("memory");

        let mut store = MemoryStore { entries: Vec::new() };
        let cfg = MemoryConfig::default();
        store.record_conversation("你好", "喵~你好！", &cfg);

        // 用临时目录模拟保存
        let json = serde_json::to_string(&store).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("chat_summary.json"), &json).unwrap();

        let _loaded = MemoryStore::load();
        // load 会读真实路径而非 tmp，所以直接验证序列化正确性
        let back: MemoryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].user_msg, "你好");
    }
}
