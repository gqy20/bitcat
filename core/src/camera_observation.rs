//! 摄像头观察记录存储模块。
//!
//! 本模块只处理摄像头帧分析后的结构化记录与可选 JPEG 落盘，不负责打开摄像头。
//! app 层负责权限、采样和 Vision 调用，core 层保持可测试的数据格式和路径规则。
//! 记录目录独立于屏幕截图，避免把用户摄像头画面和桌面截图混在一起。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 单次摄像头观察的持久化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraObservationRecord {
    pub analysis: crate::vision::VisionAnalysis,
    pub width: u32,
    pub height: u32,
    pub jpeg_size: usize,
    pub saved_frame: bool,
}

impl CameraObservationRecord {
    /// 返回供 prompt 注入或摘要消费的单行上下文。
    pub fn context_text(&self) -> String {
        self.analysis.to_context_text()
    }
}

/// 返回 `~/.bitcat/camera/` 目录。
pub fn camera_base_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".bitcat").join("camera"))
}

/// 确保当天摄像头观察目录存在。
pub fn ensure_today_dir() -> Result<PathBuf, String> {
    let base = camera_base_dir()?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dir = base.join(today);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建摄像头观察目录失败: {e}"))?;
    Ok(dir)
}

/// 保存摄像头观察记录，并按需保存原始 JPEG。
pub fn save_camera_observation(
    jpeg_bytes: &[u8],
    record: &CameraObservationRecord,
) -> Result<PathBuf, String> {
    let dir = ensure_today_dir()?;
    let prefix = chrono::Local::now().format("%H%M%S").to_string();
    if record.saved_frame {
        let jpg_path = dir.join(format!("{prefix}.jpg"));
        std::fs::write(&jpg_path, jpeg_bytes).map_err(|e| format!("保存摄像头帧失败: {e}"))?;
    }
    save_analysis_json(&dir, &prefix, record)?;
    Ok(dir.join(format!("{prefix}_analysis.json")))
}

/// 将摄像头观察记录保存为 JSON。
pub fn save_analysis_json(
    dir: &Path,
    prefix: &str,
    record: &CameraObservationRecord,
) -> Result<(), String> {
    let json_path = dir.join(format!("{prefix}_analysis.json"));
    let json =
        serde_json::to_string_pretty(record).map_err(|e| format!("序列化摄像头记录失败: {e}"))?;
    std::fs::write(&json_path, json).map_err(|e| format!("保存摄像头分析结果失败: {e}"))?;
    Ok(())
}

/// 按文件名倒序读取最近 `count` 条摄像头观察记录，并保留原始分析文件名。
pub fn list_recent_camera_observations_named(
    dir: &Path,
    count: u32,
) -> Vec<(String, CameraObservationRecord)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with("_analysis.json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(entry.path())
            && let Ok(record) = serde_json::from_str::<CameraObservationRecord>(&raw)
        {
            records.push((name, record));
        }
    }
    records.sort_by(|a, b| b.0.cmp(&a.0));
    records.into_iter().take(count as usize).collect()
}

/// 跨日期目录读取最近 `count` 条摄像头观察记录，优先取最新日期。
pub fn list_recent_camera_observations_multi_day(
    base_dir: &Path,
    count: usize,
) -> Vec<(String, String, CameraObservationRecord)> {
    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return Vec::new();
    };
    let mut day_names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() && name.len() == 10 && name.contains('-') {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    day_names.sort_by(|a, b| b.cmp(a));
    let mut results = Vec::new();
    for day in &day_names {
        if results.len() >= count {
            break;
        }
        let day_dir = base_dir.join(day);
        let needed = count - results.len();
        for (name, record) in list_recent_camera_observations_named(&day_dir, needed as u32) {
            results.push((day.clone(), name, record));
        }
    }
    results
}

/// 构建普通对话可注入的最近摄像头观察上下文。
pub fn build_recent_camera_context(count: usize, max_chars: usize) -> String {
    let base = match camera_base_dir() {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    build_recent_camera_context_with_base(count, max_chars, &base)
}

/// 同 [`build_recent_camera_context`]，但使用指定 base_dir 方便测试。
pub fn build_recent_camera_context_with_base(
    count: usize,
    max_chars: usize,
    base_dir: &Path,
) -> String {
    let records = list_recent_camera_observations_multi_day(base_dir, count);
    if records.is_empty() {
        return String::new();
    }
    let header = "[最近摄像头观察]\n";
    let footer = "[/最近摄像头观察]\n";
    let footer_chars = footer.chars().count();
    let mut result = String::from(header);
    for (i, (day, name, record)) in records.iter().rev().enumerate() {
        let time = name.trim_end_matches("_analysis.json");
        let line = format!("{}. {} {}: {}\n", i + 1, day, time, record.context_text());
        if result.chars().count() + line.chars().count() + footer_chars > max_chars {
            break;
        }
        result.push_str(&line);
    }
    result.push_str(footer);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_record_context_uses_vision_context() {
        let record = CameraObservationRecord {
            analysis: crate::vision::VisionAnalysis {
                description: "用户在电脑前".into(),
                confidence: 0.7,
                ..Default::default()
            },
            width: 640,
            height: 480,
            jpeg_size: 1024,
            saved_frame: false,
        };

        assert!(record.context_text().contains("用户在电脑前"));
    }

    #[test]
    fn build_recent_camera_context_reads_multi_day_records() {
        let tmp = tempfile::tempdir().unwrap();
        let day = tmp.path().join("2026-05-23");
        std::fs::create_dir_all(&day).unwrap();
        let record = CameraObservationRecord {
            analysis: crate::vision::VisionAnalysis {
                description: "用户在电脑前".into(),
                confidence: 0.8,
                ..Default::default()
            },
            width: 640,
            height: 480,
            jpeg_size: 1024,
            saved_frame: false,
        };
        save_analysis_json(&day, "201759", &record).unwrap();

        let ctx = build_recent_camera_context_with_base(3, 500, tmp.path());
        assert!(ctx.contains("[最近摄像头观察]"));
        assert!(ctx.contains("2026-05-23 201759"));
        assert!(ctx.contains("用户在电脑前"));
    }
}
