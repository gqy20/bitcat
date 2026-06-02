//! Vocabulary pack loading for the Snake learning mode.
//!
//! The module reads the human-editable `config/vocab_basic.yml` file and turns it into
//! a compact game payload. Keeping validation here lets the frontend stay focused on
//! gameplay while invalid packs fail before the game window opens.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const DEFAULT_YML: &str = include_str!("../../config/vocab_basic.yml");
const DEFAULT_PATH: &str = "config/vocab_basic.yml";
const MAX_ENTRIES: usize = 64;
const MAX_TEXT_CHARS: usize = 80;

/// One complete vocabulary pack as loaded from YAML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VocabPack {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_answer_count")]
    pub answer_count: u32,
    #[serde(default = "default_target_correct")]
    pub target_correct: u32,
    #[serde(default)]
    pub entries: Vec<VocabEntry>,
}

/// One vocabulary item. `distractors` are answer choices that are plausible but wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VocabEntry {
    pub id: String,
    pub term: String,
    pub meaning: String,
    #[serde(default)]
    pub distractors: Vec<String>,
    #[serde(default)]
    pub example: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_difficulty")]
    pub difficulty: u8,
}

/// Snake-specific payload embedded in `GameDef`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnakeVocabConfig {
    pub mode: String,
    pub answer_count: u32,
    pub target_correct: u32,
    pub entries: Vec<VocabEntry>,
}

impl VocabPack {
    /// Load the default vocabulary pack from exe-adjacent config, cwd config, or the embedded file.
    pub fn load_default() -> Result<Self, String> {
        let content = read_default_content();
        Self::from_yaml(&content)
    }

    /// Parse and validate a vocabulary pack from YAML text.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let mut pack: VocabPack =
            serde_yaml::from_str(yaml).map_err(|e| format!("parse vocab yaml failed: {e}"))?;
        pack.normalize();
        pack.validate()?;
        Ok(pack)
    }

    /// Convert into the game payload carried by `GameDef`.
    pub fn into_snake_config(self) -> SnakeVocabConfig {
        SnakeVocabConfig {
            mode: self.mode,
            answer_count: self.answer_count,
            target_correct: self.target_correct,
            entries: self.entries,
        }
    }

    fn normalize(&mut self) {
        self.mode = clamp_text(self.mode.trim(), 32);
        if self.mode.is_empty() {
            self.mode = default_mode();
        }
        self.answer_count = self.answer_count.clamp(2, 6);
        self.target_correct = self.target_correct.clamp(1, 50);
        self.entries.truncate(MAX_ENTRIES);
        for entry in &mut self.entries {
            entry.id = clamp_text(entry.id.trim(), 48);
            entry.term = clamp_text(entry.term.trim(), MAX_TEXT_CHARS);
            entry.meaning = clamp_text(entry.meaning.trim(), MAX_TEXT_CHARS);
            entry.example = clamp_text(entry.example.trim(), 180);
            entry.difficulty = entry.difficulty.clamp(1, 5);
            entry.distractors =
                normalize_list(std::mem::take(&mut entry.distractors), MAX_TEXT_CHARS, 8);
            entry.tags = normalize_list(std::mem::take(&mut entry.tags), 32, 8);
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.mode != "meaning_choice" {
            return Err(format!("unsupported vocab mode: {}", self.mode));
        }
        if self.entries.len() < 4 {
            return Err("vocab pack requires at least 4 entries".into());
        }
        let mut ids = HashSet::new();
        let mut meanings = HashSet::new();
        for entry in &self.entries {
            if entry.id.is_empty() || entry.term.is_empty() || entry.meaning.is_empty() {
                return Err("vocab entries require non-empty id, term, and meaning".into());
            }
            if !ids.insert(entry.id.clone()) {
                return Err(format!("duplicate vocab id: {}", entry.id));
            }
            meanings.insert(entry.meaning.clone());
            if entry.distractors.iter().any(|item| item == &entry.meaning) {
                return Err(format!("vocab distractor duplicates meaning: {}", entry.id));
            }
        }
        if meanings.len() < self.answer_count as usize {
            return Err("vocab pack needs enough distinct meanings for answer choices".into());
        }
        Ok(())
    }
}

fn read_default_content() -> String {
    read_runtime_config(DEFAULT_PATH).unwrap_or_else(|| DEFAULT_YML.to_string())
}

fn read_runtime_config(relative_path: &str) -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|dir| dir.join(relative_path)))
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| fs::read_to_string(relative_path).ok())
}

fn normalize_list(values: Vec<String>, max_chars: usize, max_len: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        let normalized = clamp_text(value.trim(), max_chars);
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        out.push(normalized);
        if out.len() >= max_len {
            break;
        }
    }
    out
}

fn clamp_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn default_mode() -> String {
    "meaning_choice".into()
}

fn default_answer_count() -> u32 {
    4
}

fn default_target_correct() -> u32 {
    12
}

fn default_difficulty() -> u8 {
    1
}

/// Return the runtime path used by the built-in basic vocabulary pack.
pub fn default_vocab_path() -> PathBuf {
    PathBuf::from(DEFAULT_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_vocab_pack_is_valid() {
        let pack = VocabPack::from_yaml(DEFAULT_YML).unwrap();
        assert!(pack.entries.len() >= 12);
        assert_eq!(pack.mode, "meaning_choice");
        assert_eq!(pack.answer_count, 4);
    }

    #[test]
    fn rejects_duplicate_ids() {
        let yaml = r#"
entries:
  - id: word
    term: word
    meaning: 词
  - id: word
    term: other
    meaning: 其他
  - id: third
    term: third
    meaning: 第三
  - id: fourth
    term: fourth
    meaning: 第四
"#;
        assert!(VocabPack::from_yaml(yaml).is_err());
    }
}
