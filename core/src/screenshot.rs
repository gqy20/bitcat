use serde::{Deserialize, Serialize};

// ---- 截图目标 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum ScreenshotTarget {
    Primary,
    #[default]
    All,
}

// ---- 截图配置 ----

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
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("SCREENSHOT_MAX_WIDTH") {
            if let Ok(w) = v.parse::<u32>() {
                cfg.max_width = w;
            }
        }
        cfg
    }

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

pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn similarity(a: u64, b: u64) -> f64 {
    1.0 - (hamming_distance(a, b) as f64 / 64.0)
}

pub fn is_similar(a: u64, b: u64, threshold: f64) -> bool {
    similarity(a, b) >= threshold
}

// ---- 截图帧数据 ----

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotRecord {
    pub description: String,
    pub hash: u64,
    pub skipped: bool,
    pub width: u32,
    pub height: u32,
    pub jpeg_size: usize,
}

pub fn screenshot_base_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法获取 HOME 目录".to_string())?;
    Ok(home.join(".ai-pad").join("screenshots"))
}

pub fn ensure_today_dir() -> Result<std::path::PathBuf, String> {
    let base = screenshot_base_dir()?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dir = base.join(&today);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建截图目录失败: {e}"))?;
    Ok(dir)
}

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

#[allow(dead_code)]
pub fn build_recent_analyses_context(count: usize, max_chars: usize) -> String {
    let base = match screenshot_base_dir() {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    build_recent_analyses_context_with_base(count, max_chars, &base)
}

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
        let line = format!("{}. {}\n", i + 1, record.description);
        if result.chars().count() + line.chars().count() + footer_chars > max_chars {
            break;
        }
        result.push_str(&line);
    }
    result.push_str(footer);
    result
}

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
