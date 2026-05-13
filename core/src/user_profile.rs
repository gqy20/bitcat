//! 用户画像配置（显式声明）
//!
//! 从 config/user.yml 加载用户主动填写的身份信息（名字、身份、偏好、语言等），
//! 构建 `[关于主人]...[/关于主人]` 格式的上下文注入 AI prompt。
//! 优先级高于 ProfileStore 的自动聚合画像：user.yml 非空时直接使用。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ---- 数据结构 ----

/// 用户显式声明的身份信息，存储在 config/user.yml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UserProfile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub preferences: Vec<String>,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub language: String,
}

const DEFAULT_YML: &str = include_str!("../../config/user.yml");

fn load_content() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("config").join("user.yml")))
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| fs::read_to_string("config/user.yml").ok())
        .unwrap_or_else(|| DEFAULT_YML.to_string())
}

fn resolve_save_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("config").join("user.yml")))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = PathBuf::from("config/user.yml");
            if p.exists() { Some(p) } else { None }
        })
        .unwrap_or_else(|| PathBuf::from("config/user.yml"))
}

impl UserProfile {
    /// 加载用户配置：exe 同目录/config/ → CWD/config/ → 编译时嵌入默认值
    pub fn load() -> Self {
        let content = load_content();
        match serde_yaml::from_str::<UserProfile>(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(error = %e, "解析 config/user.yml 失败，使用默认值");
                Self::default()
            }
        }
    }

    /// 序列化写回 config/user.yml（会覆盖注释，保存前自动备份 `.bak`）。
    pub fn save(&self) -> Result<(), String> {
        let target = resolve_save_path();
        if let Ok(old) = fs::read_to_string(&target) {
            let _ = fs::write(target.with_extension("yml.bak"), old);
        }
        let header = "# 由 8Bit Cat 设置界面生成\n\
                      # 手动编辑仍然生效，但下次保存设置会覆盖注释\n\n";
        let body = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        fs::write(&target, format!("{header}{body}"))
            .map_err(|e| format!("写入 {:?} 失败: {e}", target))
    }

    /// 返回内置默认配置（用于"重置为默认"）。
    pub fn default_builtin() -> Self {
        serde_yaml::from_str(DEFAULT_YML).expect("内置 config/user.yml 损坏")
    }

    /// 构建注入 prompt 的文本。全空字段返回空字符串。
    /// 格式与 ProfileStore.build_context() 一致：`[关于主人]...[/关于主人]`
    pub fn build_context(&self) -> String {
        let parts: Vec<&str> = vec![
            (!self.name.is_empty()).then_some(self.name.as_str()),
            (!self.role.is_empty()).then_some(self.role.as_str()),
            (!self.context.is_empty()).then_some(self.context.as_str()),
            (!self.language.is_empty()).then_some(self.role.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect();

        if parts.is_empty() && self.preferences.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        if !self.name.is_empty() {
            lines.push(format!("名字：{}", self.name));
        }
        if !self.role.is_empty() {
            lines.push(format!("身份：{}", self.role));
        }
        if !self.preferences.is_empty() {
            lines.push(format!("偏好：{}", self.preferences.join(", ")));
        }
        if !self.context.is_empty() {
            lines.push(format!("补充说明：{}", self.context));
        }
        if !self.language.is_empty() {
            lines.push(format!("语言：{}", self.language));
        }

        format!("[关于主人]\n{}\n[/关于主人]\n", lines.join("\n"))
    }

    /// 是否所有字段都为空（即用户未配置）
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
            && self.role.is_empty()
            && self.preferences.is_empty()
            && self.context.is_empty()
            && self.language.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_empty() {
        let profile = UserProfile::default();
        assert!(profile.is_empty());
        assert!(profile.build_context().is_empty());
    }

    #[test]
    fn test_build_context_with_name_and_role() {
        let profile = UserProfile {
            name: "小明".into(),
            role: "程序员".into(),
            ..Default::default()
        };
        let ctx = profile.build_context();
        assert!(ctx.contains("[关于主人]"));
        assert!(ctx.contains("名字：小明"));
        assert!(ctx.contains("身份：程序员"));
        assert!(ctx.contains("[/关于主人]"));
    }

    #[test]
    fn test_build_context_full() {
        let profile = UserProfile {
            name: "Alice".into(),
            role: "设计师".into(),
            preferences: vec!["简洁".into(), "英文".into()],
            context: "正在做 UI 改版".into(),
            language: "en-US".into(),
        };
        let ctx = profile.build_context();
        assert!(ctx.contains("名字：Alice"));
        assert!(ctx.contains("偏好：简洁, 英文"));
        assert!(ctx.contains("补充说明：正在做 UI 改版"));
    }

    #[test]
    fn test_load_builtin_parses() {
        // 内置默认 yml 应能正常解析（全空值）
        let profile = UserProfile::default_builtin();
        assert!(profile.is_empty());
    }

    #[test]
    fn test_yaml_roundtrip() {
        let original = UserProfile {
            name: "测试".into(),
            role: "QA".into(),
            preferences: vec!["详细".into()],
            context: "".into(),
            language: "".into(),
        };
        let yaml = serde_yaml::to_string(&original).unwrap();
        let back: UserProfile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.name, "测试");
        assert_eq!(back.role, "QA");
        assert_eq!(back.preferences, vec!["详细"]);
    }
}
