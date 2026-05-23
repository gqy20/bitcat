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

/// 返回 `~/.ai-pad/camera/` 目录。
pub fn camera_base_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("camera"))
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
}
