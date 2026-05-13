//! 截图管线：感知哈希去重、图像缩放、JPEG 编码与文件存储。
//!
//! 本模块提供截图数据的纯算法和 I/O 操作，不依赖任何窗口系统调用。
//! 与 `app/src/screenshot.rs` 分工：app 侧通过 BitBlt 捕获原始 BGRA 帧并调用
//! Vision API，本模块负责 dHash 感知哈希去重、resize/JPEG 编码以及
//! `~/.ai-pad/screenshots/` 下的按日存储和 7 天自动清理。
//!
//! 被截屏观察线程（`app::screenshot::screenshot_loop`）和
//! [`screen_summary`](crate::screen_summary) 模块共同消费：
//! Vision 分析结果存为 [`ScreenshotRecord`]，后续由 screen_summary 聚合注入 AI 上下文。

use serde::{Deserialize, Serialize};

// ---- 截图目标 ----

/// 截图捕获目标：仅主显示器或全部显示器（默认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum ScreenshotTarget {
    Primary,
    #[default]
    All,
}

// ---- 截图配置 ----

/// 截图管线的完整配置，来自 `config/prompts.yml` 或环境变量覆盖。
///
/// 包含目标显示器、最大宽度、JPEG 质量、定时截取间隔、dHash 去重阈值等参数。
#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotConfig {
    #[serde(default)]
    pub target: ScreenshotTarget,
    #[serde(default = "default_max_width")]
    pub max_width: u32,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    #[serde(default = "default_interval_sec")]
    pub interval_sec: u64,
    #[serde(default = "default_true")]
    pub dedup: bool,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,
    #[serde(default = "default_min_width")]
    pub min_width: u32,
    #[serde(default)]
    pub debug_resolutions: Vec<u32>,
}

fn default_max_width() -> u32 {
    960
}
fn default_jpeg_quality() -> u8 {
    80
}
fn default_interval_sec() -> u64 {
    30
}
fn default_true() -> bool {
    true
}
fn default_similarity_threshold() -> f64 {
    0.95
}
fn default_min_width() -> u32 {
    480
}
fn default_debug_resolutions() -> Vec<u32> {
    Vec::new()
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            target: ScreenshotTarget::default(),
            max_width: default_max_width(),
            jpeg_quality: default_jpeg_quality(),
            interval_sec: default_interval_sec(),
            dedup: default_true(),
            similarity_threshold: default_similarity_threshold(),
            min_width: default_min_width(),
            debug_resolutions: default_debug_resolutions(),
        }
    }
}

impl ScreenshotConfig {
    /// 从环境变量 `SCREENSHOT_MAX_WIDTH` 等读取覆盖值，未设置的项保持默认。
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("SCREENSHOT_MAX_WIDTH")
            && let Ok(w) = v.parse::<u32>()
        {
            cfg.max_width = w;
        }
        cfg
    }

    /// 校验配置合法性（JPEG 质量 1-100、max_width 非零等），失败返回描述性错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.jpeg_quality == 0 || self.jpeg_quality > 100 {
            return Err(format!(
                "jpeg_quality 必须在 1-100 之间，当前: {}",
                self.jpeg_quality
            ));
        }
        if self.max_width == 0 {
            return Err("max_width 不能为 0".into());
        }
        Ok(())
    }
}

// ---- dHash 感知哈希 ----

/// 计算灰度像素缓冲区的 dHash（差异哈希），返回 64 位指纹。
///
/// 逐行比较相邻像素亮度，将比较结果编码到 bit 位中。
/// 用于截帧去重：两帧 hash 相似度高于阈值则跳过。
pub fn perceptual_hash(pixels: &[u8], w: u32, h: u32) -> u64 {
    let mut hash: u64 = 0;
    for row in 0..h.min(8) {
        for col in 0..(w - 1).min(8) {
            let left = pixels[(row * w + col) as usize];
            let right = pixels[(row * w + col + 1) as usize];
            if right > left {
                hash |= 1u64 << (row * 8 + col);
            }
        }
    }
    hash
}

/// 两个 dHash 指纹之间的汉明距离（不同 bit 数）。
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// 两帧的相似度，1.0 表示完全相同，0.0 表示完全不同。
pub fn similarity(a: u64, b: u64) -> f64 {
    1.0 - (hamming_distance(a, b) as f64 / 64.0)
}

/// 判断两帧是否在给定阈值内相似。
pub fn is_similar(a: u64, b: u64, threshold: f64) -> bool {
    similarity(a, b) >= threshold
}

// ---- 截图帧数据 ----

/// 单帧 BGRA 像素数据及其尺寸。
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 显示器的逻辑矩形区域（像素），用于多显示器拼接时的坐标计算。
#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// 将多帧水平拼接为一张完整图像，高度取最大值，不足部分填充透明。
pub fn stitch_horizontal(frames: &[&CapturedFrame]) -> CapturedFrame {
    if frames.is_empty() {
        return CapturedFrame {
            pixels: vec![],
            width: 0,
            height: 0,
        };
    }
    let total_width: u32 = frames.iter().map(|f| f.width).sum();
    let max_height: u32 = frames.iter().map(|f| f.height).max().unwrap_or(0);
    let mut pixels = vec![0u8; (total_width * max_height * 4) as usize];
    let mut x_offset: u32 = 0;
    for frame in frames {
        for y in 0..frame.height {
            let src_start = (y * frame.width * 4) as usize;
            let src_end = src_start + (frame.width * 4) as usize;
            let dst_start = (y * total_width * 4 + x_offset * 4) as usize;
            pixels[dst_start..dst_start + src_end - src_start]
                .copy_from_slice(&frame.pixels[src_start..src_end]);
        }
        x_offset += frame.width;
    }
    CapturedFrame {
        pixels,
        width: total_width,
        height: max_height,
    }
}

// ---- Resize + JPEG 编码 ----

/// 将 BGRA 像素缓冲区按比例缩小到 `max_width` 以内，输出 RGB 数据。
///
/// 若原始宽度不超过 `max_width` 则直接转换颜色空间不缩放。
/// 使用 Triangle 滤波以保证缩放质量。
pub fn resize_bgra(
    bgra: &[u8],
    w: u32,
    h: u32,
    max_width: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let expected = (w * h * 4) as usize;
    if bgra.len() < expected {
        return Err(format!(
            "BGRA buffer too small: expected {} bytes, got {}",
            expected,
            bgra.len()
        ));
    }
    let (out_w, out_h) = if w <= max_width {
        (w, h)
    } else {
        let scale = max_width as f64 / w as f64;
        (max_width, (h as f64 * scale).round() as u32)
    };
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for chunk in bgra[..expected].chunks_exact(4) {
        rgba.push(chunk[2]);
        rgba.push(chunk[1]);
        rgba.push(chunk[0]);
        rgba.push(chunk[3]);
    }
    use image::{ImageBuffer, RgbaImage, imageops};
    let img: RgbaImage =
        ImageBuffer::from_raw(w, h, rgba).ok_or_else(|| "无法创建图像缓冲区".to_string())?;
    let resized = imageops::resize(&img, out_w, out_h, imageops::FilterType::Triangle);
    let mut rgb = Vec::with_capacity((out_w * out_h * 3) as usize);
    for pixel in resized.pixels() {
        rgb.push(pixel[0]);
        rgb.push(pixel[1]);
        rgb.push(pixel[2]);
    }
    Ok((rgb, out_w, out_h))
}

/// 将 RGB 像素编码为 JPEG 字节流，quality 范围 1-100。
pub fn encode_jpeg(rgb: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>, String> {
    use image::codecs::jpeg::JpegEncoder;
    use image::{ImageBuffer, RgbImage};
    use std::io::Cursor;
    let img: RgbImage = ImageBuffer::from_raw(w, h, rgb.to_vec())
        .ok_or_else(|| "无法创建 RGB 图像缓冲区".to_string())?;
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder)
        .map_err(|e| format!("JPEG 编码失败: {e}"))?;
    Ok(buf.into_inner())
}

// ---- 截图存储 ----

/// 截图分析结果的持久化记录，包含 Vision 分析、感知哈希、尺寸和跳过标记。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotRecord {
    pub analysis: crate::vision::VisionAnalysis,
    pub hash: u64,
    pub skipped: bool,
    pub width: u32,
    pub height: u32,
    pub jpeg_size: usize,
}

impl ScreenshotRecord {
    /// 返回 Vision 分析的描述文本。
    pub fn description(&self) -> &str {
        &self.analysis.description
    }

    /// 生成供 prompt 注入用的单行上下文文本。
    pub fn context_text(&self) -> String {
        self.analysis.to_context_text()
    }
}

/// 返回 `~/.ai-pad/screenshots/` 路径。
pub fn screenshot_base_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("screenshots"))
}

/// 确保当天日期子目录存在并返回其路径（如 `~/.ai-pad/screenshots/2025-06-01/`）。
pub fn ensure_today_dir() -> Result<std::path::PathBuf, String> {
    let base = screenshot_base_dir()?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dir = base.join(&today);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建截图目录失败: {e}"))?;
    Ok(dir)
}

/// 将 JPEG 字节和分析记录写入当天目录（`HHMMSS.jpg` + `HHMMSS_analysis.json`）。
pub fn save_screenshot(
    jpeg_bytes: &[u8],
    record: &ScreenshotRecord,
) -> Result<std::path::PathBuf, String> {
    let dir = ensure_today_dir()?;
    let prefix = chrono::Local::now().format("%H%M%S").to_string();
    let jpg_path = dir.join(format!("{prefix}.jpg"));
    std::fs::write(&jpg_path, jpeg_bytes).map_err(|e| format!("保存截图失败: {e}"))?;
    save_analysis_json(&dir, &prefix, "", record)?;
    Ok(jpg_path)
}

/// 将分析记录序列化为 JSON 写入指定目录，文件名由 prefix + suffix 组成。
pub fn save_analysis_json(
    dir: &std::path::Path,
    prefix: &str,
    suffix: &str,
    record: &ScreenshotRecord,
) -> Result<(), String> {
    let json_path = dir.join(format!("{prefix}{suffix}_analysis.json"));
    let json =
        serde_json::to_string_pretty(record).map_err(|e| format!("序列化分析结果失败: {e}"))?;
    std::fs::write(&json_path, json).map_err(|e| format!("保存分析结果失败: {e}"))?;
    Ok(())
}

/// 按文件名倒序读取最近 count 条分析记录（单目录）。
pub fn list_recent_analyses(dir: &std::path::Path, count: u32) -> Vec<ScreenshotRecord> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut records: Vec<(String, ScreenshotRecord)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with("_analysis.json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(entry.path())
            && let Ok(record) = serde_json::from_str::<ScreenshotRecord>(&raw)
        {
            records.push((name, record));
        }
    }
    records.sort_by(|a, b| b.0.cmp(&a.0));
    records
        .into_iter()
        .take(count as usize)
        .map(|(_, r)| r)
        .collect()
}

/// 跨日期目录读取最近 count 条分析记录，优先取最新的日期。
#[allow(dead_code)]
pub fn list_recent_analyses_multi_day(
    base_dir: &std::path::Path,
    count: usize,
) -> Vec<(String, ScreenshotRecord)> {
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
        for record in list_recent_analyses(&day_dir, needed as u32) {
            results.push((day.clone(), record));
        }
    }
    results
}

/// 构建最近截图观察的 prompt 上下文片段（`[最近截图观察]...[/最近截图观察]`）。
#[allow(dead_code)]
pub fn build_recent_analyses_context(count: usize, max_chars: usize) -> String {
    let base = match screenshot_base_dir() {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    build_recent_analyses_context_with_base(count, max_chars, &base)
}

/// 同 [`build_recent_analyses_context`]，但使用指定的 base_dir（方便测试）。
pub fn build_recent_analyses_context_with_base(
    count: usize,
    max_chars: usize,
    base_dir: &std::path::Path,
) -> String {
    let records = list_recent_analyses_multi_day(base_dir, count);
    if records.is_empty() {
        return String::new();
    }
    let header = "[最近截图观察]\n";
    let footer = "[/最近截图观察]\n";
    let footer_chars = footer.chars().count();
    let mut result = String::from(header);
    for (i, (_day, record)) in records.iter().rev().enumerate() {
        let line = format!("{}. {}\n", i + 1, record.context_text());
        if result.chars().count() + line.chars().count() + footer_chars > max_chars {
            break;
        }
        result.push_str(&line);
    }
    result.push_str(footer);
    result
}

/// 删除超过 keep_days 天的日期子目录，返回删除数量。
pub fn cleanup_old_screenshots(keep_days: u64) -> Result<u32, String> {
    let base = screenshot_base_dir()?;
    if !base.exists() {
        return Ok(0);
    }
    let cutoff = chrono::Local::now() - chrono::Duration::days(keep_days as i64);
    let cutoff_str: String = cutoff.format("%Y-%m-%d").to_string();
    let mut removed = 0u32;
    let entries = std::fs::read_dir(&base).map_err(|e| format!("读取截图目录失败: {e}"))?;
    for entry in entries.flatten() {
        let name_str: String = entry.file_name().to_string_lossy().into_owned();
        if name_str < cutoff_str && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
